//TODO
#![allow(dead_code)]

use super::message::{message::Message, pipe_ext::PipeExt};
use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::{collections::HashMap, path::PathBuf};
use tokio::{fs::read, select, sync::mpsc};
use uuid::Uuid;
use wasmer::{Instance, Module, Store};
use wasmer_wasi::{Pipe, WasiState};

pub type FunctionID = Uuid;
pub type InstanceID = Uuid;

#[derive(Deserialize)]
pub struct Config {
    pub id: FunctionID,
    // TODO: key must not contain `=` and both must not contain `null` byte
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

#[derive(PartialEq)]
pub enum FunctionStatus {
    Loaded,
    Running, // TODO: add started time here
}

// TODO: Add status for storing current status of the function
pub struct Function {
    pub instance_id: InstanceID,
    status: FunctionStatus,
    pipes: FunctionPipes,
    config: Config,
    store: Store,
    module: Module,
}

type Input = mpsc::UnboundedSender<Message>;
type Output = mpsc::UnboundedReceiver<Message>;

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
            status: FunctionStatus::Loaded,
            pipes,
            config,
            store,
            module,
        })
    }

    fn create_std_io(&mut self) -> Result<(Input, Output)> {
        let mut stdout_reader = self.pipes.stdout.clone().to_message_reader();
        let mut stdin_writer = self.pipes.stdin.clone().to_message_writer();

        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Message>();
        let (output_tx, output_rx) = mpsc::unbounded_channel::<Message>();

        tokio::spawn(async move {
            // TODO: stop if function is not running
            loop {
                select! {
                    Some(message) = input_rx.recv() => {
                        // TODO: handle error
                        stdin_writer.send(message).await.unwrap();
                    }
                    Some(message) = stdout_reader.next() => {
                        // TODO: handle error
                        //              pipe_err decoding_err
                        let item = message.unwrap().unwrap();
                        output_tx.send(item).unwrap();
                    }
                }
            }
        });

        Ok((input_tx, output_rx))
    }

    pub async fn start(&mut self) -> Result<(Input, Output)> {
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

        //TODO: configure `Builder` of tokio for huge blocking tasks
        tokio::task::spawn_blocking(move || {
            let start = instance.exports.get_function("_start").unwrap();
            start.call(&mut self.store, &[]).unwrap();
            todo!()
        });
        self.create_std_io()
    }
}
