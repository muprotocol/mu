use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_yaml;

#[derive(Deserialize)]
pub struct Template {
    pub name: String,
    pub lang: String,
    files: Vec<File>,
    //TODO: Check if order of commands is preserved or not.
    commands: Vec<Command>,
}

#[derive(Deserialize)]
struct File {
    path: PathBuf,
    contents: FileContents,
    args: Vec<String>,
}

#[derive(Deserialize)]
enum FileContents {
    String(String),
    File(PathBuf),
}

#[derive(Deserialize)]
enum Command {
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
            let path = destination.join(&file.path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let mut contents = match file.contents {
                FileContents::String(ref s) => s.clone(),
                FileContents::File(_) => unimplemented!(),
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
    let dir_entries = std::fs::read_dir("./template")?;
    let mut templates = vec![];

    for item in dir_entries {
        match item {
            Ok(i) if i.file_type()?.is_file() => {
                let file = std::fs::File::open(i.path()).context("template file")?;
                let template: Template = serde_yaml::from_reader(file)?;
                templates.push(template);
            }
            Ok(_) => continue,
            Err(e) => bail!("failed to ready template directory contents: {e:?}"),
        }
    }
    Ok(templates)
}
