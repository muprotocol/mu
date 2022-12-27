use super::error::Error;
use mu_stack::{FunctionRuntime, StackID};

use anyhow::Result;
use bytes::Bytes;
use mailbox_processor::ReplyChannel;
use serde::Deserialize;
use std::{
    collections::HashMap,
    fmt::Display,
    io::{BufReader, BufWriter},
    path::PathBuf,
};
use tokio::{sync::oneshot::error::TryRecvError, task::JoinHandle};
use uuid::Uuid;
use wasmer_middlewares::metering::MeteringPoints;
use wasmer_wasi::Pipe;

pub type ExecuteFunctionRequest<'a> = musdk_common::incoming_message::ExecuteFunction<'a>;
pub type ExecuteFunctionResponse = musdk_common::outgoing_message::FunctionResult<'static>;

pub trait FunctionProvider: Send {
    fn get(&self, id: &FunctionID) -> Option<&FunctionDefinition>;
    fn add_function(&mut self, function: FunctionDefinition);
    fn remove_function(&mut self, id: &FunctionID);
    fn get_function_names(&self, stack_id: &StackID) -> Vec<String>;
}

#[derive(Debug)]
pub struct InvokeFunctionRequest {
    pub function_id: FunctionID,
    pub request: ExecuteFunctionRequest<'static>,
    pub reply: ReplyChannel<Result<ExecuteFunctionResponse, Error>>,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct InstanceID {
    pub function_id: FunctionID,
    pub instance_id: Uuid,
}

impl Display for InstanceID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.function_id, self.instance_id)
    }
}

impl InstanceID {
    pub fn generate_random(function_id: FunctionID) -> Self {
        Self {
            function_id,
            instance_id: Uuid::new_v4(),
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct FunctionID {
    pub stack_id: StackID,
    pub function_name: String,
}

impl Display for FunctionID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.stack_id, self.function_name)
    }
}

pub type FunctionSource = Bytes;

#[derive(Clone, Debug)]
pub struct FunctionDefinition {
    pub id: FunctionID,
    pub source: FunctionSource,
    pub runtime: FunctionRuntime,

    // TODO: key must not contain `=` and both must not contain `null` byte
    pub envs: HashMap<String, String>,
    pub memory_limit: byte_unit::Byte,
}

impl FunctionDefinition {
    pub fn new(
        id: FunctionID,
        source: FunctionSource,
        runtime: FunctionRuntime,
        envs: impl IntoIterator<
            IntoIter = impl Iterator<Item = (String, String)>,
            Item = (String, String),
        >,
        memory_limit: byte_unit::Byte,
    ) -> Self {
        let envs: HashMap<String, String> = envs.into_iter().collect();
        Self {
            id,
            source,
            runtime,
            envs,
            memory_limit,
        }
    }
}

#[derive(Debug)]
pub struct FunctionIO {
    pub stdin: BufWriter<Pipe>,
    pub stdout: BufReader<Pipe>,
    pub stderr: BufReader<Pipe>,
}

#[derive(Debug)]
pub struct FunctionHandle {
    pub join_handle: JoinHandle<Result<MeteringPoints, (super::error::Error, MeteringPoints)>>,
    is_finished_rx: tokio::sync::oneshot::Receiver<()>,
    is_finished: bool,
    pub io: FunctionIO,
}

impl FunctionHandle {
    pub fn new(
        join_handle: JoinHandle<Result<MeteringPoints, (super::error::Error, MeteringPoints)>>,
        is_finished_rx: tokio::sync::oneshot::Receiver<()>,
        io: FunctionIO,
    ) -> Self {
        Self {
            join_handle,
            is_finished_rx,
            io,
            is_finished: false,
        }
    }

    pub fn is_finished(&mut self) -> bool {
        if self.is_finished {
            true
        } else {
            let is_finished = match self.is_finished_rx.try_recv() {
                // if the second half was somehow dropped without sending a value, we can still
                // assume the function is "finished" in the sense that it's no longer running.
                Ok(()) | Err(TryRecvError::Closed) => true,
                Err(TryRecvError::Empty) => false,
            };

            if is_finished {
                self.is_finished = true;
            }
            is_finished
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct RuntimeConfig {
    pub cache_path: PathBuf,
}

pub type InstructionsCount = u64;
