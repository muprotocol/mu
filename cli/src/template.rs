use std::{
    collections::HashMap,
    fmt::Display,
    path::{Path, PathBuf},
    process::Stdio,
};

use anyhow::{bail, Context, Result};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};

use crate::mu_manifest::MUManifest;

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

#[derive(Deserialize)]
pub struct Template {
    pub lang: Language,
    files: Vec<File>,
}

#[derive(Deserialize, Serialize, Clone, Copy)]
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
    pub fn load_builtins() -> Result<Vec<TemplateSet>> {
        let mut template_sets = vec![];
        let template_set_files = TemplateSets::iter().filter_map(|i| TemplateSets::get(&i));

        for template_set in template_set_files {
            let template_set =
                serde_yaml::from_slice(&template_set.data).context("reading template sets")?;
            template_sets.push(template_set);
        }

        Ok(template_sets)
    }
}

impl Template {
    pub fn create(&self, path: &Path, args: HashMap<String, String>) -> Result<()> {
        //TODO: check for a valid rust/other-langs project name.
        let Some(project_name) = args.get("name") else {
            bail!("project name was not given in arguments, `name`");
        };

        std::fs::create_dir(path)?;

        for file in self.files.iter() {
            let path = path.join(&file.path); //TODO: replace args in path too
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let mut contents = match file.contents {
                FileContent::String(ref s) => s.clone(),
                FileContent::File(_) => unimplemented!(),
            };

            for arg in &file.args {
                let Some(value) = args.get(arg) else {
                    bail!("template argument `{arg}` was not found in provided arguments") 
                };

                contents = contents.replace(&format!("{{{{{arg}}}}}"), value);
            }

            std::fs::write(path, contents)?;
        }

        //TODO: create .gitignore file
        let git_result = std::process::Command::new("git")
            .arg("init")
            .current_dir(path)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .map_err(|e| anyhow::format_err!("git init failed: {}", e.to_string()))?;
        if !git_result.status.success() {
            eprintln!("Failed to automatically initialize a new git repository");
        }

        MUManifest::new(project_name.clone(), self.lang).write(path)
    }
}
