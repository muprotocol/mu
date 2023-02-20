use std::{
    collections::HashMap,
    io::Read,
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
pub struct MuManifest {
    name: String,
    version: String,
    #[serde(
        serialize_with = "custom_stack_id_serialization::serialize",
        deserialize_with = "custom_stack_id_serialization::deserialize"
    )]
    pub dev_id: StackID,
    services: Vec<Service>,
}

impl MuManifest {
    pub fn read<R: Read>(reader: &mut R) -> Result<Self> {
        serde_yaml::from_reader(reader).context("Invalid mu manifest file")
    }

    fn all_functions(&self) -> impl Iterator<Item = &Function> {
        self.services.iter().filter_map(|s| {
            if let Service::Function(f) = s {
                Some(f)
            } else {
                None
            }
        })
    }

    pub fn build_all(&self, build_mode: BuildMode, project_root: &Path) -> Result<()> {
        for f in self.all_functions() {
            println!("Building {}", f.name);
            f.build(build_mode, project_root)?;
        }

        Ok(())
    }

    pub fn generate_stack_manifest(
        &self,
        build_mode: BuildMode,
        generation_mode: ArtifactGenerationMode,
    ) -> Result<Stack> {
        let overridden_envs = std::env::vars()
            .filter_map(|(k, v)| {
                if k.len() > 3 && k.starts_with("MU_") {
                    Some((k[3..].to_string(), v))
                } else {
                    None
                }
            })
            .collect::<HashMap<String, String>>();

        let services = self
            .services
            .iter()
            .map(|s| {
                anyhow::Ok(match s {
                    Service::Database(d) => mu_stack::Service::Database(d.clone()),
                    Service::Gateway(g) => mu_stack::Service::Gateway(g.clone()),
                    Service::Function(f) => {
                        let binary = match generation_mode {
                            ArtifactGenerationMode::LocalRun => {
                                f.wasm_module_path(build_mode).display().to_string()
                            }
                            ArtifactGenerationMode::Publish => bail!("Not implemented"),
                        };

                        let mut env = f.env.clone();

                        if let ArtifactGenerationMode::LocalRun = generation_mode {
                            env.extend(f.env_dev.iter().map(|(k, v)| (k.clone(), v.clone())));
                            env.extend(overridden_envs.clone());
                        }

                        mu_stack::Service::Function(mu_stack::Function {
                            name: f.name.clone(),
                            binary,
                            runtime: f.runtime,
                            env,
                            memory_limit: f.memory_limit,
                        })
                    }
                })
            })
            .bcollect()?;

        Ok(Stack {
            name: self.name.clone(),
            version: self.version.clone(),
            services,
        })
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
    pub lang: Language,
    pub runtime: AssemblyRuntime,
    pub env: HashMap<String, String>,
    pub env_dev: HashMap<String, String>,
    #[serde(serialize_with = "custom_byte_unit_serialization::serialize")]
    pub memory_limit: byte_unit::Byte,
}

impl Function {
    fn wasm_module_path(&self, build_mode: BuildMode) -> PathBuf {
        match self.lang {
            Language::Rust => {
                let cargo_target_dir = std::env::var_os("CARGO_TARGET_DIR")
                    .map(|x| {
                        let path: &Path = x.as_ref();
                        path.to_owned()
                    })
                    .unwrap_or({
                        let mut root = self.root_dir();
                        root.push("target");
                        root
                    });

                let build_mode = match build_mode {
                    BuildMode::Debug => "debug",
                    BuildMode::Release => "release",
                };

                cargo_target_dir.join(format!("wasm32-wasi/{build_mode}/{}.wasm", self.name))
            }
        }
    }

    pub fn root_dir(&self) -> PathBuf {
        format!("functions/{}", self.name).into()
    }

    pub fn build(&self, build_mode: BuildMode, project_root: &Path) -> Result<()> {
        let create_cmd = |cmd, args: &[&str]| {
            let mut cmd = std::process::Command::new(cmd);
            for arg in args {
                cmd.arg(arg);
            }
            cmd
        };

        let commands = match self.lang {
            Language::Rust => {
                let mut manifest = project_root.to_owned();
                manifest.push(self.root_dir());
                manifest.push("Cargo.toml");
                match build_mode {
                    BuildMode::Debug => [
                        create_cmd("rustup", &["target", "add", "wasm32-wasi"]),
                        create_cmd(
                            "cargo",
                            &[
                                "build",
                                "--target",
                                "wasm32-wasi",
                                "--manifest-path",
                                manifest.display().to_string().as_str(),
                            ],
                        ),
                    ],

                    BuildMode::Release => [
                        create_cmd("rustup", &["target", "add", "wasm32-wasi"]),
                        create_cmd(
                            "cargo",
                            &[
                                "build",
                                "--release",
                                "--target",
                                "wasm32-wasi",
                                "--manifest-path",
                                manifest.display().to_string().as_str(),
                            ],
                        ),
                    ],
                }
            }
        };

        for mut cmd in commands {
            let exit = cmd
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .output()
                .map_err(|e| anyhow::format_err!("{}", e.to_string()))?;

            if !exit.status.success() {
                bail!("Failed to run pre-build script")
            }
        }

        Ok(())
    }
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

mod custom_byte_unit_serialization {
    use serde::Serializer;

    pub fn serialize<S>(item: &byte_unit::Byte, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = item.get_appropriate_unit(true).to_string();
        serializer.serialize_str(&s)
    }
}
