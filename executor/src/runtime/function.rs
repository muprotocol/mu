//TODO
#![allow(dead_code)]

use super::message::{pipe_ext::PipeExt, Message};
use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::{collections::HashMap, path::PathBuf};
use tokio::{
    select,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use uuid::Uuid;
use wasmer::{Instance, Module, Store};
use wasmer_wasi::{Pipe, WasiState};

pub type FunctionID = Uuid;
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

struct FunctionPipes {
    pub stdin: Pipe,
    pub stdout: Pipe,
    pub stderr: Pipe,
}

pub struct FunctionIO {
    input: mpsc::UnboundedSender<Message>,
    output: mpsc::UnboundedReceiver<Message>,
}

pub struct Function {
    pipes: FunctionPipes,
    config: Config,
    store: Store,
    module: Module,
}

impl Function {
    fn create_io(
        pipes: &FunctionPipes,
        mut task_stoped: oneshot::Receiver<()>,
    ) -> Result<FunctionIO> {
        let mut stdout_reader = pipes.stdout.clone().to_message_reader();
        let mut stdin_writer = pipes.stdin.clone().to_message_writer();

        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Message>();
        let (output_tx, output_rx) = mpsc::unbounded_channel::<Message>();

        tokio::spawn(async move {
            loop {
                select! {
                    _r = &mut task_stoped => {
                        break //TODO log
                    }
                    message = input_rx.recv() => {
                        // TODO: handle error
                        match message {
                            Some(m) => stdin_writer.send(m).await.unwrap(),
                            None => break, //TODO: log
                        }
                    }
                    message = stdout_reader.next() => {
                        match message {
                            Some(Ok(m)) => {
                                // TODO: log error (decoding_err) and notify user
                                let item = m.unwrap();
                                output_tx.send(item).unwrap();
                            },
                            Some(Err(_)) | None => break //TODO: log
                        }
                    }
                }
            }
        });

        Ok(FunctionIO {
            input: input_tx,
            output: output_rx,
        })
    }

    //TODO: configure `Builder` of tokio for huge blocking tasks
    pub fn start(mut self) -> Result<(JoinHandle<()>, FunctionIO)> {
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

        let (task_stoped_tx, task_stoped_rx) = oneshot::channel();

        let join_handle = tokio::task::spawn_blocking(move || {
            //TODO: bubble up the error to outer task
            let start = instance.exports.get_function("_start").unwrap();
            start.call(&mut self.store, &[]).unwrap();
            task_stoped_tx.send(()).unwrap();
            //TODO: report usage to runtime
        });

        let io = Self::create_io(&self.pipes, task_stoped_rx)?;

        Ok((join_handle, io))
    }
}
