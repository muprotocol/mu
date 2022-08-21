//TODO
#![allow(dead_code)]

use super::types::ID;
use anyhow::Result;
use serde::Deserialize;
use std::{collections::HashMap, path::PathBuf};
use tokio::task::JoinHandle;
use wasmer::{Instance, Module, Store};
use wasmer_wasi::{Pipe, WasiState};

pub type FunctionID = ID<Function>;
pub type FunctionSource = Vec<u8>;

#[derive(Deserialize, Clone)]
pub struct Config {
    pub id: FunctionID,
    // TODO: key must not contain `=` and both must not contain `null` byte
    pub envs: HashMap<String, String>,
    pub module_path: PathBuf,
}

impl Config {
    pub fn new(id: FunctionID, envs: HashMap<String, String>, path: PathBuf) -> Self {
        Self {
            id,
            envs,
            module_path: path,
        }
    }
}

pub struct FunctionDefinition {
    pub id: FunctionID,
    source: FunctionSource,
    config: Config,
}

impl FunctionDefinition {
    pub fn new(id: FunctionID, source: FunctionSource, config: Config) -> Self {
        Self { id, source, config }
    }

    pub async fn create_function(&self) -> Result<Function> {
        let store = Store::default();
        let module = Module::from_binary(&store, &self.source)?;

        let pipes = FunctionPipes {
            stdin: Pipe::new(),
            stdout: Pipe::new(),
            stderr: Pipe::new(),
        };

        Ok(Function {
            pipes,
            config: self.config.clone(),
            store,
            module,
        })
    }
}

pub struct FunctionPipes {
    pub stdin: Pipe,
    pub stdout: Pipe,
    pub stderr: Pipe,
}

pub struct Function {
    pipes: FunctionPipes,
    config: Config,
    store: Store,
    module: Module,
}

impl Function {
    //TODO: configure `Builder` of tokio for huge blocking tasks
    pub fn start(mut self) -> Result<(JoinHandle<()>, FunctionPipes)> {
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

        let join_handle = tokio::task::spawn_blocking(move || {
            //TODO: bubble up the error to outer task
            let start = instance.exports.get_function("_start").unwrap();
            start.call(&mut self.store, &[]).unwrap();
            //TODO: report usage to runtime
        });

        Ok((join_handle, self.pipes))
    }
}
