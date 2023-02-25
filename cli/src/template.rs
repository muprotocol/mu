use std::{
    collections::HashMap,
    fmt::Display,
    path::{Path, PathBuf},
    process::Stdio,
    str::FromStr,
};

use anyhow::{anyhow, Result};
use itertools::Itertools;
use mu_stack::StackID;
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use thiserror::Error;

//TODO: Currently we embed the `templates` folder in our binary, but it's good to be able to read
//other templates from user local system.

#[derive(RustEmbed)]
#[folder = "templates"]
struct TemplateSets;

#[derive(Deserialize)]
pub struct TemplateSet {
    pub name: String,
    pub templates: Vec<Template>,
}

impl Display for TemplateSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let langs = concat_string(self.templates.iter().map(|t| t.lang.to_string()));
        write!(f, "{: ^10}|{: ^15}", self.name, langs)
    }
}

#[derive(Deserialize)]
pub struct Template {
    pub lang: Language,
    files: Vec<File>,
}

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Language {
    Rust,
}

impl Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Language::Rust => "Rust",
        };
        std::fmt::Display::fmt(s, f)
    }
}

impl FromStr for Language {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rust" => Ok(Self::Rust),
            _ => Err(anyhow!("Invalid language")),
        }
    }
}

#[derive(Deserialize)]
pub struct File {
    pub path: PathBuf,
    pub contents: FileContent,
    pub args: Vec<String>,
}

#[derive(Deserialize)]
pub enum FileContent {
    String(String),
    File(PathBuf),
}

impl TemplateSet {
    pub fn load_builtin() -> Result<Vec<TemplateSet>> {
        TemplateSets::iter()
            .filter_map(|i| TemplateSets::get(&i))
            .map(|t| serde_yaml::from_slice(&t.data).map_err(Into::into))
            .collect::<Result<Vec<TemplateSet>>>()
    }

    pub fn create(
        &self,
        lang: Language,
        path: &Path,
        args: HashMap<String, String>,
    ) -> Result<(), TemplateError> {
        //TODO: check for a valid rust/other-langs project name.
        std::fs::create_dir_all(path)
            .map_err(|e| TemplateError::FailedToCreateDirectory(path.to_path_buf(), e))?;

        if let Some(template) = self.templates.iter().find(|t| t.lang == lang) {
            template.create_files(path, args)?;
        } else {
            return Err(TemplateError::LanguageNotSupported {
                requested: lang,
                available: self.templates.iter().map(|t| t.lang).collect(),
            });
        }

        let git_result = std::process::Command::new("git")
            .arg("init")
            .current_dir(path)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .map_err(|e| TemplateError::FailedToRunCommand("git init".to_string(), e))?;
        if !git_result.status.success() {
            eprintln!("Failed to automatically initialize a new git repository");
        }

        Ok(())
    }
}

impl Template {
    pub fn create_files(
        &self,
        path: &Path,
        mut args: HashMap<String, String>,
    ) -> Result<(), TemplateError> {
        // `dev_id` for MuManifest
        let bytes = rand::random::<[u8; 32]>();
        let dev_id = StackID::SolanaPublicKey(bytes);
        args.insert("dev_id".into(), dev_id.to_string());

        for file in self.files.iter() {
            let path = PathBuf::from(Self::replace_args(
                path.join(&file.path)
                    .to_str()
                    .ok_or(TemplateError::NonUnicodePathNotSupported)?,
                &file.args,
                &args,
            )?);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| TemplateError::FailedToCreateDirectory(parent.to_path_buf(), e))?;
            }

            let mut contents = match file.contents {
                FileContent::String(ref s) => s.clone(),
                FileContent::File(_) => unimplemented!(),
            };

            contents = Self::replace_args(&contents, &file.args, &args)?;

            std::fs::write(&path, contents)
                .map_err(|e| TemplateError::FailedToCreateFile(path, e))?;
        }
        Ok(())
    }

    fn replace_args(
        template: &str,
        arg_names: &[String],
        args: &HashMap<String, String>,
    ) -> Result<String, TemplateError> {
        let mut res = template.to_string();
        for arg in arg_names {
            let Some(value) = args.get(arg) else {
                return Err(TemplateError::ArgumentMissing(arg.to_string()));
            };

            res = res.replace(&format!("{{{{{arg}}}}}"), value);
        }
        Ok(res)
    }
}

#[derive(Debug, Error)]
pub enum TemplateError {
    #[error("Argument {0} is missing in arguments")]
    ArgumentMissing(String),

    #[error("This template does not support `{requested}`, available languages are: {}",
        concat_string(available.iter().map(ToString::to_string)))]
    LanguageNotSupported {
        requested: Language,
        available: Vec<Language>,
    },

    #[error("Failed to create directory {0}: {}", .1.to_string())]
    FailedToCreateDirectory(PathBuf, std::io::Error),

    #[error("Failed to create file {0}: {}", .1.to_string())]
    FailedToCreateFile(PathBuf, std::io::Error),

    #[error("Failed to run `{0}` command: {}", .1.to_string())]
    FailedToRunCommand(String, std::io::Error),

    #[error("Non-unicode paths are not supported")]
    NonUnicodePathNotSupported,
}

fn concat_string<I: Iterator<Item = String>>(items: I) -> String {
    Itertools::intersperse(items, ", ".into()).collect::<String>()
}
