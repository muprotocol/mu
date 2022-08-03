use super::message::message::{InputMessage, OutputMessage};
use anyhow::{bail, Result};
use serde::Deserialize;
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::PathBuf,
};
use tokio::fs::read;
use uuid::Uuid;
use wasmer::{Instance, Module, Store};
use wasmer_wasi::{Pipe, WasiState};

pub type FunctionID = Uuid;
pub type InstanceID = Uuid;

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct Config {
    pub id: FunctionID,
    //TODO: key must not contain `=` and both must not contain `null` byte
    envs: HashMap<String, String>,
    path: PathBuf,
}

#[allow(dead_code)]
impl Config {
    pub fn new(id: FunctionID, envs: HashMap<String, String>, path: PathBuf) -> Self {
        Self { id, envs, path }
    }
}

#[allow(dead_code)]
struct FunctionPipes {
    pub stdin: Pipe,
    pub stdout: Pipe,
    pub stderr: Pipe,
}

#[allow(dead_code)]
pub struct Function {
    pub instance_id: InstanceID,
    pipes: FunctionPipes,
    config: Config,
    store: Store,
    module: Module,
}

#[allow(dead_code)]
impl Function {
    pub async fn load(config: Config) -> Result<Self> {
        let src = read(&config.path).await?;

        let store = Store::default();
        let module = Module::from_binary(&store, &src)?;

        let pipes = FunctionPipes {
            stdin: Pipe::new(),
            stdout: Pipe::new(),
            stderr: Pipe::new(),
        };

        Ok(Self {
            instance_id: Uuid::new_v4(),
            pipes,
            config,
            store,
            module,
        })
    }

    async fn write_stdin(&mut self, input: InputMessage) -> Result<()> {
        let message = serde_json::to_string(&input)?;
        let bytes_written = self.pipes.stdin.write(message.as_bytes())?;
        let message_bytes = message.as_bytes().len();
        if bytes_written == message_bytes {
            Ok(())
        } else {
            bail!(format!(
                "can not write the whole input message, only {} of {} was written.",
                bytes_written, message_bytes
            ))
        }
    }

    async fn read_stdout(&mut self) -> Result<OutputMessage> {
        let mut buf = String::new();
        self.pipes.stdout.read_to_string(&mut buf)?;
        serde_json::from_str(&buf).map_err(Into::into)
    }

    pub async fn run<'a>(&mut self, request: InputMessage) -> Result<OutputMessage> {
        let name = self.module.name().unwrap_or("module");
        let wasi_env = WasiState::new(name)
            .stdin(Box::new(self.pipes.stdin.clone()))
            .stdout(Box::new(self.pipes.stdout.clone()))
            .stderr(Box::new(self.pipes.stderr.clone()))
            .envs(self.config.envs.clone())
            .finalize(&mut self.store)?;

        let import_object = wasi_env.import_object(&mut self.store, &self.module)?;

        let instance = Instance::new(&mut self.store, &self.module, &import_object)?;

        let memory = instance.exports.get_memory("memory")?;

        wasi_env
            .data_mut(&mut self.store)
            .set_memory(memory.clone());

        let start = instance.exports.get_function("_start")?;

        //TODO: We can check if all of request was written
        self.write_stdin(request).await?;

        if let Err(e) = start.call(&mut self.store, &[]) {
            Err(e.into())
        } else {
            self.read_stdout().await
        }
    }
}
