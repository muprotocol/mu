use super::{error::Error, function::Pipe};
use mu_stack::{AssemblyRuntime, StackID};

use anyhow::Result;
use bytes::Bytes;
use mailbox_processor::ReplyChannel;
use serde::Deserialize;
use std::{collections::HashMap, fmt::Display, path::PathBuf};
use tokio::{sync::oneshot::error::TryRecvError, task::JoinHandle};
use wasmer_middlewares::metering::MeteringPoints;

pub(super) type ExecuteFunctionRequest<'a> = musdk_common::incoming_message::ExecuteFunction<'a>;
pub(super) type ExecuteFunctionResponse = musdk_common::outgoing_message::FunctionResult<'static>;

pub trait AssemblyProvider: Send {
    fn get(&self, id: &AssemblyID) -> Option<&AssemblyDefinition>;
    fn add_function(&mut self, function: AssemblyDefinition);
    fn remove_function(&mut self, id: &AssemblyID);
    fn get_function_names(&self, stack_id: &StackID) -> Vec<String>;
}

#[derive(Debug)]
pub struct InvokeFunctionRequest {
    pub assembly_id: AssemblyID,
    pub request: ExecuteFunctionRequest<'static>,
    pub reply: ReplyChannel<Result<ExecuteFunctionResponse, Error>>,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct InstanceID {
    pub function_id: AssemblyID,
    pub instance_id: u64,
}

impl Display for InstanceID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.function_id, self.instance_id)
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct AssemblyID {
    pub stack_id: StackID,
    pub assembly_name: String,
}

impl Display for AssemblyID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.stack_id, self.assembly_name)
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct FunctionID {
    pub assembly_id: AssemblyID,
    pub function_name: String,
}

impl Display for FunctionID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.assembly_id, self.function_name)
    }
}

pub type AssemblySource = Bytes;

#[derive(Clone, Debug)]
pub struct AssemblyDefinition {
    pub id: AssemblyID,
    pub source: AssemblySource,
    pub runtime: AssemblyRuntime,

    // TODO: key must not contain `=` and both must not contain `null` byte
    pub envs: HashMap<String, String>,
    pub memory_limit: byte_unit::Byte,
}

impl AssemblyDefinition {
    pub fn new(
        id: AssemblyID,
        source: AssemblySource,
        runtime: AssemblyRuntime,
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
    pub stdin: Pipe,
    pub stdout: Pipe,
    pub stderr: Pipe,
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
    pub include_function_logs: bool,
}

pub type InstructionsCount = u64;
