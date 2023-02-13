use std::{
    collections::HashMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
};

use anyhow::{bail, Context, Result};
use beau_collector::BeauCollector;
use mu_stack::{AssemblyRuntime, Database, Gateway, Stack, StackID};
use serde::{Deserialize, Serialize};

use crate::template::Language;

pub const MU_MANIFEST_FILE_NAME: &str = "mu.yaml";
#[allow(dead_code)]
pub const STACK_MANIFEST_FILE_NAME: &str = "stack.yaml";

#[derive(Serialize, Deserialize)]
pub struct MUManifest {
    #[serde(
        serialize_with = "custom_stack_id_serialization::serialize",
        deserialize_with = "custom_stack_id_serialization::deserialize"
    )]
    pub test_id: StackID,
    name: String,
    lang: Language,
    version: String,
    services: Vec<Service>,
}

impl MUManifest {
    pub fn new(name: String, lang: Language) -> Self {
        let bytes = rand::random::<[u8; 32]>();
        let test_id = StackID::SolanaPublicKey(bytes);

        MUManifest {
            test_id,
            name,
            lang,
            version: "0.1".to_string(),
            services: vec![],
        }
    }

    pub fn write<W: Write>(&self, writer: &mut W) -> Result<()> {
        serde_yaml::to_writer(writer, &self).context("Failed to write mu manifest")?;
        Ok(())
    }

    pub fn read<R: Read>(reader: &mut R) -> Result<Self> {
        serde_yaml::from_reader(reader).context("Invalid mu manifest file")
    }

    //TODO: support multiple function in a single manifest
    pub fn wasm_module_path(&self, build_mode: BuildMode) -> PathBuf {
        match self.lang {
            Language::Rust => {
                let cargo_target_dir =
                    std::env::var_os("CARGO_TARGET_DIR").unwrap_or("target".into());
                let cargo_target_dir = Path::new(&cargo_target_dir);

                let build_mode = match build_mode {
                    BuildMode::Debug => "debug",
                    BuildMode::Release => "release",
                };

                cargo_target_dir.join(format!("wasm32-wasi/{build_mode}/{}.wasm", self.name))
            }
        }
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
            bail!("Failed to run pre-build script")
        }

        let exit = build
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .map_err(|e| anyhow::format_err!("{}", e.to_string()))?;

        if !exit.status.success() {
            bail!("Failed to build the project")
        }

        Ok(())
    }

    pub fn generate_stack_manifest(
        &self,
        build_mode: BuildMode,
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
                        binary: self
                            .upload_function(self.wasm_module_path(build_mode), generation_mode)?,
                        runtime: f.runtime,
                        env: f.env,
                        memory_limit: f.memory_limit,
                    }),
                })
            })
            .bcollect()?;

        Ok(Stack {
            name: self.name.clone(),
            version: self.version.clone(),
            services,
        })
    }

    pub fn add_services(mut self, services: &mut Vec<Service>) -> Self {
        self.services.append(services);
        self
    }

    fn upload_function(
        &self,
        wasm_module_path: PathBuf,
        generation_mode: ArtifactGenerationMode,
    ) -> Result<String> {
        match generation_mode {
            ArtifactGenerationMode::LocalRun => Ok(wasm_module_path.display().to_string()),
            ArtifactGenerationMode::Publish => unimplemented!("don't have storage service yet"),
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

mod custom_stack_id_serialization {
    use std::str::FromStr;

    use mu_stack::StackID;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(item: &StackID, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = item.to_string();
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<StackID, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        StackID::from_str(&s).map_err(|_| serde::de::Error::custom("invalid StackID"))
    }
}
