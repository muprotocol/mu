use std::{
    collections::HashMap,
    fmt::Display,
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    str::FromStr,
};

use anyhow::{anyhow, bail, Context, Result};
use beau_collector::BeauCollector;
use mu_stack::{
    stack_id_as_string_serialization, AssemblyRuntime, Gateway, NameAndDelete, Stack, StackID,
};
use serde::{Deserialize, Serialize};

pub const MU_MANIFEST_FILE_NAME: &str = "mu.yaml";

#[derive(Serialize, Deserialize)]
pub struct MuManifest {
    name: String,
    version: String,
    #[serde(
        serialize_with = "stack_id_as_string_serialization::serialize",
        deserialize_with = "stack_id_as_string_serialization::deserialize"
    )]
    pub dev_id: StackID,
    services: Vec<Service>,
}

impl MuManifest {
    pub fn read<R: Read>(reader: &mut R) -> Result<Self> {
        serde_yaml::from_reader(reader).context("Invalid mu manifest file")
    }

    #[allow(dead_code)]
    fn all_functions(&self) -> impl Iterator<Item = &Function> {
        self.services.iter().filter_map(|s| {
            if let Service::Function(f) = s {
                Some(f)
            } else {
                None
            }
        })
    }

    #[allow(dead_code)]
    pub fn build_all(&self, build_mode: BuildMode, project_root: &Path) -> Result<()> {
        for f in self.all_functions() {
            println!("Building {}", f.name);
            f.build(build_mode, project_root)?;
        }

        Ok(())
    }

    #[cfg(feature = "dev-env")]
    pub fn generate_stack_manifest_for_local_run(&self, build_mode: BuildMode) -> Result<Stack> {
        self.generate_stack_manifest(build_mode, ArtifactGenerationMode::LocalRun, |p| Ok(p))
    }

    pub fn generate_stack_manifest_for_publish<F>(&self, function_uploader: F) -> Result<Stack>
    where
        F: Fn(String) -> Result<String>,
    {
        self.generate_stack_manifest(
            BuildMode::Release,
            ArtifactGenerationMode::LocalRun,
            function_uploader,
        )
    }

    fn generate_stack_manifest<F>(
        &self,
        build_mode: BuildMode,
        generation_mode: ArtifactGenerationMode,
        function_uploader: F,
    ) -> Result<Stack>
    where
        F: Fn(String) -> Result<String>, // Wasm module path -> Function source url/path
    {
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
                    Service::KeyValueTable(k) => mu_stack::Service::KeyValueTable(k.clone()),
                    Service::Gateway(g) => mu_stack::Service::Gateway(g.clone()),
                    Service::Function(f) => {
                        let wasm_module_path = f.wasm_module_path(build_mode).display().to_string();
                        let binary = function_uploader(wasm_module_path)?;

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

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum Service {
    KeyValueTable(NameAndDelete),
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
                    .unwrap_or(self.root_dir().join("target"));

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

    #[allow(dead_code)]
    pub fn build(&self, build_mode: BuildMode, project_root: &Path) -> Result<()> {
        let create_cmd = |cmd, args: &[&str]| {
            let mut cmd = std::process::Command::new(cmd);
            for arg in args {
                cmd.arg(arg);
            }
            cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
            cmd
        };

        let commands = match self.lang {
            Language::Rust => {
                let manifest = project_root.join(self.root_dir()).join("Cargo.toml");
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
                .output()
                .map_err(|e| anyhow::format_err!("{}", e.to_string()))?;

            if !exit.status.success() {
                bail!("Failed to run build script")
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

pub fn read_manifest() -> Result<(MuManifest, PathBuf)> {
    let mut path = std::env::current_dir()?;

    loop {
        let manifest_path = path.join(MU_MANIFEST_FILE_NAME);
        if manifest_path.try_exists()? {
            let mut file = std::fs::File::open(&manifest_path)?;
            return Ok((MuManifest::read(&mut file)?, path));
        }
        let Some(parent) = path.parent() else {
            break
        };
        path = parent.into();
    }

    bail!(
        "Not in a mu project, `{}` file not found.",
        MU_MANIFEST_FILE_NAME
    );
}
