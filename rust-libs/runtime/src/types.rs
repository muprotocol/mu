use crate::FunctionLoadingError;

use super::{
    error::{Error, Result},
    function::Pipe,
};

use mu_stack::{AssemblyID, AssemblyRuntime};

use bytes::Bytes;
use mailbox_processor::ReplyChannel;
use serde::Deserialize;
use std::{collections::HashMap, fmt::Display, marker::PhantomData, path::PathBuf};
use tokio::{sync::oneshot::error::TryRecvError, task::JoinHandle};
use wasmer_middlewares::metering::MeteringPoints;

pub(super) type ExecuteFunctionRequest<'a> = musdk_common::incoming_message::ExecuteFunction<'a>;
pub(super) type ExecuteFunctionResponse = musdk_common::outgoing_message::FunctionResult<'static>;

#[derive(Debug)]
pub struct InvokeFunctionRequest {
    pub assembly_id: AssemblyID,
    pub request: ExecuteFunctionRequest<'static>,
    pub reply: ReplyChannel<Result<ExecuteFunctionResponse>>,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub(super) struct InstanceID {
    pub function_id: AssemblyID,
    pub instance_id: u64,
}

impl Display for InstanceID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.function_id, self.instance_id)
    }
}

#[derive(Clone, Debug)]
pub struct AssemblyDefinition {
    pub id: AssemblyID,
    pub source: Bytes,
    pub runtime: AssemblyRuntime,

    pub envs: HashMap<String, String>,
    pub memory_limit: byte_unit::Byte,

    _make_me_private: PhantomData<()>,
}

impl AssemblyDefinition {
    pub fn try_new(
        id: AssemblyID,
        source: Bytes,
        runtime: AssemblyRuntime,
        envs: impl IntoIterator<
            IntoIter = impl Iterator<Item = (String, String)>,
            Item = (String, String),
        >,
        memory_limit: byte_unit::Byte,
    ) -> Result<Self> {
        let envs: HashMap<String, String> = envs.into_iter().collect();
        for e in &envs {
            if e.0.contains('=') {
                return Err(Error::FunctionLoadingError(
                    FunctionLoadingError::InvalidAssemblyDefinition(
                        "Env key cannot contain '=' character".to_string(),
                    ),
                ));
            }
            if e.0.contains('\0') {
                return Err(Error::FunctionLoadingError(
                    FunctionLoadingError::InvalidAssemblyDefinition(
                        "Env key cannot contain null character".to_string(),
                    ),
                ));
            }
            if e.1.contains('\0') {
                return Err(Error::FunctionLoadingError(
                    FunctionLoadingError::InvalidAssemblyDefinition(
                        "Env value cannot contain null character".to_string(),
                    ),
                ));
            }
        }
        Ok(Self {
            id,
            source,
            runtime,
            envs,
            memory_limit,
            _make_me_private: PhantomData,
        })
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
    pub join_handle: JoinHandle<Result<MeteringPoints, (Error, MeteringPoints)>>,
    is_finished_rx: tokio::sync::oneshot::Receiver<()>,
    is_finished: bool,
    pub io: FunctionIO,
}

impl FunctionHandle {
    pub fn new(
        join_handle: JoinHandle<Result<MeteringPoints, (Error, MeteringPoints)>>,
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
