use std::{
    collections::HashMap,
    fmt::Display,
    io::{BufReader, BufWriter},
};

use anyhow::Result;

use super::message::{gateway::GatewayResponse, Message};
use crate::mu_stack::{FunctionRuntime, StackID};

use mailbox_processor::ReplyChannel;
use tokio::task::JoinHandle;
use uuid::Uuid;
use wasmer_middlewares::metering::MeteringPoints;
use wasmer_wasi::Pipe;

/// This is the FunctionProvider that should cache functions if needed.
pub trait FunctionProvider: Send {
    fn get(&self, id: &FunctionID) -> Option<&FunctionDefinition>;
    fn add_function(&mut self, function: FunctionDefinition);
    fn remove_function(&mut self, id: &FunctionID);
    fn get_function_names(&self, stack_id: &StackID) -> Vec<String>;
}

pub type FunctionUsage = u64; // # of executed instructions

#[derive(Debug)]
pub struct InvokeFunctionRequest {
    // TODO: not needed in public interface
    pub function_id: FunctionID,
    pub message: Message,
    pub reply: ReplyChannel<Result<(GatewayResponse, FunctionUsage)>>,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct InstanceID {
    pub function_id: FunctionID,
    instance_id: Uuid,
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

pub type FunctionSource = Vec<u8>;

#[derive(Clone, Debug)]
pub struct FunctionDefinition {
    pub id: FunctionID,
    pub source: FunctionSource,
    pub runtime: FunctionRuntime,

    // TODO: key must not contain `=` and both must not contain `null` byte
    pub envs: HashMap<String, String>,
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
    ) -> Self {
        let envs: HashMap<String, String> = envs.into_iter().collect();
        Self {
            id,
            source,
            runtime,
            envs,
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
    pub join_handle: JoinHandle<MeteringPoints>,
    pub io: FunctionIO,
}
