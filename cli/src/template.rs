use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use rust_embed::RustEmbed;
use serde::Deserialize;

//TODO: Currently we embed the `templates` folder in our binary, but it's good to be able to read
//other templates from user local system.

#[derive(RustEmbed)]
#[folder = "templates"]
struct Templates;

#[derive(Deserialize)]
pub struct Template {
    pub name: String,
    pub lang: String,
    files: Vec<File>,
    //TODO: Check if order of commands is preserved or not.
    commands: Vec<Command>,
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

#[derive(Deserialize)]
pub enum Command {
    Prefix(String),
    Postfix(String),
}

impl Command {
    pub fn is_prefix(&self) -> bool {
        match self {
            Command::Prefix(_) => true,
            Command::Postfix(_) => false,
        }
    }

    pub fn is_postfix(&self) -> bool {
        !self.is_prefix()
    }

    pub fn command(&self) -> &str {
        match self {
            Command::Prefix(ref s) => s,
            Command::Postfix(ref s) => s,
        }
    }
}

impl Template {
    pub fn build<'a>(&self, destination: &Path, args: HashMap<String, String>) -> Result<()> {
        std::fs::create_dir_all(destination)?;

        for cmd in self.commands.iter().filter(|c| c.is_prefix()) {
            std::process::Command::new(cmd.command()).spawn()?;
        }

        for file in self.files.iter() {
            let path = destination.join(&file.path); //TODO: replace args in path too
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

        for cmd in self.commands.iter().filter(|c| c.is_postfix()) {
            std::process::Command::new(cmd.command()).spawn()?;
        }

        Ok(())
    }
}

pub fn read_templates() -> Result<Vec<Template>> {
    let mut templates = vec![];
    let template_files = Templates::iter().filter_map(|i| Templates::get(&i));

    for template in template_files {
        let template = serde_yaml::from_slice(&template.data).context("reading template")?;
        templates.push(template);
    }

    Ok(templates)
}
