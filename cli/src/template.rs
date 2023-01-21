use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_yaml;

#[derive(Deserialize)]
struct Template {
    name: String,
    lang: String,
    files: Vec<File>,
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
}

impl Template {
    pub fn build(&self, destination: &Path) -> Result<()> {
        std::fs::create_dir_all(destination)?;

        for command in self.commands.iter().filter(|c| c.is_prefix()) {
            std::process::Command::new(command).spawn()?;
        }

        for file in self.files.iter() {
            let path = destination.join(&file.path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            match file.contents {
                FileContents::String(ref s) => std::fs::write(path, s)?,
                FileContents::File(_) => unimplemented!(),
            }
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
