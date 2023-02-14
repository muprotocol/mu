use std::{
    collections::HashMap,
    fmt::Display,
    path::{Path, PathBuf},
    process::Stdio,
    str::FromStr,
};

use anyhow::{anyhow, Result};
use itertools::Itertools;
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::mu_manifest::{self, MUManifest, Service};

//TODO: Currently we embed the `templates` folder in our binary, but it's good to be able to read
//other templates from user local system.

#[derive(RustEmbed)]
#[folder = "templates"]
struct TemplateSets;

#[derive(Deserialize)]
pub struct TemplateSet {
    pub name: String,
    pub templates: Vec<Template>,
    pub services: Vec<Service>,
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

    #[allow(unstable_name_collisions)]
    pub fn print(&self) {
        let langs = self
            .templates
            .iter()
            .map(|t| t.lang.to_string())
            .intersperse(", ".to_string())
            .collect::<String>();

        println!("{: ^10}|{: ^15}", self.name, langs);
    }

    pub fn print_all(sets: &[Self]) {
        println!("Available templates:\n");
        println!("{: ^10}|{: ^15}", "Name", "Languages");
        println!("{}", "-".repeat(25));

        sets.iter().for_each(Self::print);
    }

    pub fn create(
        &self,
        lang: Language,
        path: &Path,
        args: HashMap<String, String>,
    ) -> Result<(), TemplateError> {
        //TODO: check for a valid rust/other-langs project name.
        let Some(project_name) = args.get("name") else {
            return Err(TemplateError::ArgumentMissing("name".to_string()));
        };

        std::fs::create_dir(path)
            .map_err(|e| TemplateError::FailedToCreateDirectory(path.to_path_buf(), e))?;

        if let Some(template) = self.templates.iter().find(|t| t.lang == lang) {
            template.create_files(path, &args)?;
        } else {
            return Err(TemplateError::LanguageNotSupported {
                requested: lang,
                available: self.templates.iter().map(|t| t.lang).collect(),
            });
        }

        //TODO: create .gitignore file
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

        let mu_manifest_path = path.join(mu_manifest::MU_MANIFEST_FILE_NAME);

        //TODO: use `File::create_new` when it got stabilized
        let mut mu_manifest_file = std::fs::File::create(&mu_manifest_path)
            .map_err(|e| TemplateError::FailedToCreateFile(mu_manifest_path, e))?;

        MUManifest::new(project_name.clone(), lang)
            .add_services(&mut self.services.clone())
            .write(&mut mu_manifest_file)
            .map_err(TemplateError::FailedToWriteMuManifest)?;

        Ok(())
    }
}

impl Template {
    pub fn create_files(
        &self,
        path: &Path,
        args: &HashMap<String, String>,
    ) -> Result<(), TemplateError> {
        for file in self.files.iter() {
            //TODO: replace args in path too
            let path = path.join(&file.path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| TemplateError::FailedToCreateDirectory(parent.to_path_buf(), e))?;
            }

            let mut contents = match file.contents {
                FileContent::String(ref s) => s.clone(),
                FileContent::File(_) => unimplemented!(),
            };

            for arg in &file.args {
                let Some(value) = args.get(arg) else {
                    return Err(TemplateError::ArgumentMissing(arg.to_string()));
                };

                contents = contents.replace(&format!("{{{{{arg}}}}}"), value);
            }

            std::fs::write(&path, contents)
                .map_err(|e| TemplateError::FailedToCreateFile(path, e))?;
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum TemplateError {
    #[error("Argument {0} is missing in arguments")]
    ArgumentMissing(String),

    #[error("This template does not support `{requested}`, available languages are: {}", available.iter().fold(String::new(), |a, s| format!("{a}, {s}")))]
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

    #[error("Failed to write mu manifest: {0:?}")]
    FailedToWriteMuManifest(anyhow::Error),
}
