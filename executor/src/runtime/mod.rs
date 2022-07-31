use anyhow::Result;
use error::Error;
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

mod error;

type FunctionID = Uuid;
type InstanceID = Uuid;

#[derive(Deserialize)]
pub struct Config {
    pub id: FunctionID,
    //TODO: key must not contain `=` and both must not contain `null` byte
    envs: HashMap<String, String>,
    path: PathBuf,
}

impl Config {
    pub fn new(id: FunctionID, envs: HashMap<String, String>, path: PathBuf) -> Self {
        Self { id, envs, path }
    }
}

struct FunctionPipes {
    pub stdin: Pipe,
    pub stdout: Pipe,
    pub stderr: Pipe,
}

struct Function {
    pub instance_id: InstanceID,
    pipes: FunctionPipes,
    config: Config,
    store: Store,
    module: Module,
}

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

    async fn write_stdin(&mut self, buf: &[u8]) -> Result<usize> {
        self.pipes.stdin.write(buf).map_err(Into::into)
    }

    async fn read_stdout(&mut self, buf: &mut String) -> Result<usize> {
        self.pipes.stdout.read_to_string(buf).map_err(Into::into)
    }

    pub async fn run(&mut self, request: &[u8]) -> Result<String> {
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
            let mut buf = String::new();
            self.read_stdout(&mut buf).await.unwrap(); //TODO: handle error
            Ok(buf)
        }
    }
}

//TODO: use metrics and MemoryUsage so we can report usage of memory and CPU time.
#[derive(Default)]
pub struct MuRuntime {
    //TODO: use Vec<Function> and hold more than one function at a time so we can load balance
    // over funcs.
    instances: HashMap<InstanceID, Function>,
}

impl MuRuntime {
    pub async fn load_function(&mut self, config: Config) -> Result<()> {
        if self.instances.get(&config.id).is_none() {
            let id = config.id;
            let function = Function::load(config).await?;
            self.instances.insert(id, function);
        }
        Ok(())
    }

    pub async fn run_function(&mut self, id: Uuid, request: &[u8]) -> Result<String> {
        if let Some(f) = self.instances.get_mut(&id) {
            f.run(request).await
        } else {
            Err(Error::FunctionNotFound(id).into())
        }
    }
}
