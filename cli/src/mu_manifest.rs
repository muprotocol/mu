use std::{
    collections::HashMap,
    io::{Read, Write},
    path::PathBuf,
    process::Stdio,
};

use anyhow::{Context, Result};
use beau_collector::BeauCollector;
use mu_stack::{AssemblyRuntime, Database, Gateway, Stack, StackID, STACK_ID_SIZE};
use serde::{Deserialize, Serialize};

use crate::template::Language;

pub const MU_MANIFEST_FILE_NAME: &str = "mu.yaml";
pub const STACK_MANIFEST_FILE_NAME: &str = "stack.yaml";

#[derive(Serialize, Deserialize)]
pub struct MUManifest {
    pub id: StackID,
    name: String,
    lang: Language,
    version: String,
    services: Vec<Service>,
}

impl MUManifest {
    pub fn new(name: String, lang: Language) -> Result<Self> {
        let id = StackID::try_from_bytes(&rand::random::<[u8; STACK_ID_SIZE]>())
            .context("failed to generate new id")?;

        Ok(MUManifest {
            id,
            name,
            lang,
            version: "0.1".to_string(),
            services: vec![],
        })
    }

    pub fn write<W: Write>(&self, writer: &mut W) -> Result<()> {
        serde_yaml::to_writer(writer, &self)?;
        Ok(())
    }

    pub fn read<R: Read>(reader: &mut R) -> Result<Self> {
        serde_yaml::from_reader(reader).map_err(Into::into)
    }

    //TODO: support multiple function in a single manifest
    pub fn wasm_module_path(&self) -> PathBuf {
        let path = match self.lang {
            Language::Rust => {
                format!("target/wasm32-wasi/release/{}.wasm", self.name)
            }
        };

        PathBuf::from(path)
    }

    pub fn build_project(&self, build_mode: BuildMode) -> Result<()> {
        let create_cmd = |cmd, args: &[&str]| {
            let mut cmd = std::process::Command::new(cmd);
            for arg in args {
                cmd.arg(arg);
            }
            cmd
        };

        let (mut pre_build, mut build) = match (self.lang, build_mode) {
            (Language::Rust, BuildMode::Debug) => (
                create_cmd("rustup", &["target", "add", "wasm32-wasi"]),
                create_cmd("cargo", &["build", "--target", "wasm32-wasi"]),
            ),

            (Language::Rust, BuildMode::Release) => (
                create_cmd("rustup", &["target", "add", "wasm32-wasi"]),
                create_cmd("cargo", &["build", "--release", "--target", "wasm32-wasi"]),
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

    pub fn generate_stack_manifest(
        &self,
        generation_mode: ArtifactGenerationMode,
    ) -> Result<Stack> {
        let services = self
            .services
            .clone()
            .into_iter()
            .map(|s| {
                anyhow::Ok(match s {
                    Service::Database(d) => mu_stack::Service::Database(d),
                    Service::Gateway(g) => mu_stack::Service::Gateway(g),
                    Service::Function(f) => mu_stack::Service::Function(mu_stack::Function {
                        name: f.name,
                        binary: self.upload_function(self.wasm_module_path(), generation_mode)?,
                        runtime: f.runtime,
                        env: f.env,
                        memory_limit: f.memory_limit,
                    }),
                })
            })
            .bcollect()?;

        Ok(Stack {
            name: self.name,
            version: self.version,
            services,
        })
    }

    fn upload_function(
        &self,
        _wasm_module_path: PathBuf,
        generation_mode: ArtifactGenerationMode,
    ) -> Result<String> {
        match generation_mode {
            ArtifactGenerationMode::LocalRun => {
                //TODO: copy wasm file to http server serving directory
                Ok(format!("http://localhost:8080/{}.wasm", self.name))
            }
            ArtifactGenerationMode::Publish => unimplemented!(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum Service {
    Database(Database),
    Gateway(Gateway),
    Function(Function),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Function {
    pub name: String,
    pub runtime: AssemblyRuntime,
    pub env: HashMap<String, String>,
    pub test_env: HashMap<String, String>,
    pub memory_limit: byte_unit::Byte,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum BuildMode {
    Debug,
    Release,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum ArtifactGenerationMode {
    LocalRun,
    Publish,
}

impl Default for BuildMode {
    fn default() -> Self {
        Self::Debug
    }
}

impl Default for ArtifactGenerationMode {
    fn default() -> Self {
        Self::LocalRun
    }
}
