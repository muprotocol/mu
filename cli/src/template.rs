use std::{
    borrow::Cow,
    collections::HashMap,
    fmt::Display,
    path::{Path, PathBuf},
    process::Stdio,
};

use anyhow::{bail, Context, Result};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};

//TODO: Currently we embed the `templates` folder in our binary, but it's good to be able to read
//other templates from user local system.

#[derive(RustEmbed)]
#[folder = "templates"]
struct Templates;

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
pub struct Template {
    pub name: String,
    pub lang: Language,
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
    pub fn create<'a>(&self, path: &Path, args: HashMap<String, String>) -> Result<()> {
        //TODO: check for a valid rust/other-langs project name.
        let Some(project_name) = args.get("name") else {
            bail!("project name was not given in arguments, `name`");
        };

        std::fs::create_dir(path)?;

        for cmd in self.commands.iter().filter(|c| c.is_prefix()) {
            std::process::Command::new(cmd.command()).spawn()?;
        }

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

        for cmd in self.commands.iter().filter(|c| c.is_postfix()) {
            std::process::Command::new(cmd.command()).spawn()?;
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

        MUManifest::new(project_name.clone(), self.lang).cretae(path)
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

#[derive(Serialize, Deserialize)]
pub struct MUManifest {
    pub name: String,
    pub lang: Language,
}

impl MUManifest {
    pub fn new(name: String, lang: Language) -> Self {
        MUManifest { name, lang }
    }

    pub fn cretae(&self, path: &Path) -> Result<()> {
        let file = std::fs::File::options()
            .write(true)
            .create_new(true)
            .open(path.join("Mu.yaml"))?;

        serde_yaml::to_writer(file, &self)?;
        Ok(())
    }

    pub fn read_file(path: Option<&Path>) -> Result<Self> {
        let path = match path {
            Some(p) => Cow::Borrowed(p),
            None => Cow::Owned(std::env::current_dir()?),
        }
        .join("Mu.yaml");

        if !path.try_exists()? {
            bail!("Not in a mu project, Mu.yaml not found.");
        }

        let file = std::fs::File::open(path)?;
        serde_yaml::from_reader(file).map_err(Into::into)
    }

    pub fn build_project(&self) -> Result<()> {
        let create_cmd = |cmd, args: &[&str]| {
            let mut cmd = std::process::Command::new(cmd);
            for arg in args {
                cmd.arg(arg);
            }
            cmd
        };

        let (mut pre_build, mut build, _wasm_module) = match self.lang {
            Language::Rust => (
                create_cmd("rustup", &["target", "add", "wasm32-wasi"]),
                create_cmd("cargo", &["build", "--release", "--target", "wasm32-wasi"]),
                format!("target/wasm32-wasi/release/{}.wasm", self.name),
            ),
        };

        let exit = pre_build
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .map_err(|e| anyhow::format_err!("{}", e.to_string()))?;

        if !exit.status.success() {
            eprintln!("pre-build command failed");
        }

        let exit = build
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .map_err(|e| anyhow::format_err!("{}", e.to_string()))?;

        if !exit.status.success() {
            eprintln!("build command failed");
        }

        Ok(())
    }
}
