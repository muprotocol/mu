use std::{collections::HashMap, sync::Arc};

use super::{
    error::{Error, FunctionLoadingError},
    function,
    memory::create_memory,
    types::{ExecuteFunctionRequest, ExecuteFunctionResponse, FunctionHandle, InstanceID},
};
use crate::{error::FunctionRuntimeError, Usage};

use anyhow::anyhow;
use log::{error, log, trace, Level};
use mu_stack::AssemblyID;
use musdk_common::{
    incoming_message::IncomingMessage,
    outgoing_message::{LogLevel, OutgoingMessage},
};
use wasmer::{CompilerConfig, Module, RuntimeError, Store};
use wasmer_compiler_llvm::LLVM;
use wasmer_middlewares::{metering::MeteringPoints, Metering};

const FUNCTION_LOG_TARGET: &str = "mu_function";

pub fn create_store(memory_limit: byte_unit::Byte) -> Result<Store, Error> {
    let mut compiler_config = LLVM::default();

    let metering = Arc::new(Metering::new(u64::MAX, |_| 1));
    compiler_config.push_middleware(metering);

    let memory = create_memory(memory_limit).map_err(|_| {
        Error::FunctionLoadingError(FunctionLoadingError::RequestedMemorySizeTooBig)
    })?;

    Ok(Store::new_with_tunables(compiler_config, memory))
}

fn create_usage(
    db_read: u64,
    db_write: u64,
    instructions_count: u64,
    memory: byte_unit::Byte,
) -> Usage {
    let memory_megabytes = memory
        .get_adjusted_unit(byte_unit::ByteUnit::MB)
        .get_value();
    let memory_megabytes = memory_megabytes.floor() as u64;

    Usage {
        db_strong_reads: 0,
        db_strong_writes: 0,
        db_weak_reads: db_read,
        db_weak_writes: db_write,
        function_instructions: instructions_count,
        memory_megabytes,
    }
}

fn metering_point_to_instructions_count(points: &MeteringPoints) -> u64 {
    match points {
        MeteringPoints::Exhausted => u64::MAX,
        MeteringPoints::Remaining(p) => u64::MAX - p,
    }
}

pub trait InstanceState {}

pub struct Loaded {
    store: Store,
    envs: HashMap<String, String>,
    module: Module,
}
impl InstanceState for Loaded {}

pub struct Running {
    handle: FunctionHandle,
    io_state: IOState,

    //TODO: Refactor these to `week` and `strong` when we had database replication
    database_write_count: u64,
    database_read_count: u64,
}

impl InstanceState for Running {}

#[derive(Debug)]
pub struct Instance<S: InstanceState> {
    id: InstanceID,
    state: S,
    memory_limit: byte_unit::Byte,
    include_logs: bool,
}

impl Instance<Loaded> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        assembly_id: AssemblyID,
        instance_id: u64,
        envs: HashMap<String, String>,
        store: Store,
        module: Module,
        memory_limit: byte_unit::Byte,
        include_logs: bool,
    ) -> Self {
        let state = Loaded {
            store,
            envs,
            module,
        };

        Instance {
            id: InstanceID {
                function_id: assembly_id,
                instance_id,
            },
            state,
            memory_limit,
            include_logs,
        }
    }

    pub fn start(self) -> Result<Instance<Running>, Error> {
        trace!(
            "starting instance {} of function {}",
            self.id.instance_id,
            self.id.function_id
        );

        let handle = function::start(self.state.store, &self.state.module, self.state.envs)?;
        let state = Running {
            handle,
            io_state: IOState::Idle,
            database_write_count: 0,
            database_read_count: 0,
        };
        Ok(Instance {
            id: self.id,
            state,
            memory_limit: self.memory_limit,
            include_logs: self.include_logs,
        })
    }
}

impl Instance<Running> {
    #[inline]
    pub async fn run_request(
        self,
        request: ExecuteFunctionRequest<'static>,
    ) -> Result<(ExecuteFunctionResponse, Usage), (Error, Usage)> {
        tokio::task::spawn_blocking(move || self.inner_run_request(request))
            .await
            .map_err(|_| {
                (
                    Error::Internal(anyhow!("can not run function task to end")),
                    Default::default(),
                )
            })?
    }

    pub fn is_finished(&mut self) -> bool {
        self.state.handle.is_finished()
    }

    fn write_message(&mut self, message: IncomingMessage) -> Result<(), Error> {
        message
            .write(&mut self.state.handle.io.stdin)
            .map_err(|e| {
                error!("failed to write data to function: {e}");
                Error::Internal(anyhow!("failed to write data to function {e}",))
            })?;

        Ok(())
    }

    fn read_message(&mut self) -> Result<OutgoingMessage<'static>, Error> {
        OutgoingMessage::read(&mut self.state.handle.io.stdout).map_err(Error::FailedToReadMessage)
    }

    fn wait_to_finish_and_get_usage(self) -> Result<Usage, (Error, Usage)> {
        tokio::runtime::Handle::current()
            .block_on(self.state.handle.join_handle)
            .map(move |metering_points| {
                let usage = |m| {
                    create_usage(
                        self.state.database_read_count,
                        self.state.database_write_count,
                        metering_point_to_instructions_count(m),
                        self.memory_limit,
                    )
                };

                match metering_points {
                    Ok(m) => Ok(usage(&m)),
                    Err((e, m)) => Err((e, usage(&m))),
                }
            })
            .map_err(|_| {
                (
                    Error::Internal(anyhow!("failed to run function task to end")),
                    Default::default(),
                )
            })?
    }

    fn inner_run_request(
        mut self,
        request: ExecuteFunctionRequest<'static>,
    ) -> Result<(ExecuteFunctionResponse, Usage), (Error, Usage)> {
        trace!("Running {}", &self.id);

        if self.is_finished() {
            trace!(
                "Instance {:?} is already exited before sending request",
                &self.id
            );

            return Err((
                Error::FunctionRuntimeError(FunctionRuntimeError::FunctionEarlyExit(
                    RuntimeError::new("Function Early Exit"),
                )),
                Default::default(),
            ));
        }

        self.write_message(IncomingMessage::ExecuteFunction(request))
            .map_err(|e| (e, Default::default()))?;
        self.state.io_state = IOState::Processing;

        loop {
            // TODO: overall timeout for functions

            // TODO: make this async? Possible, but needs work in Borsh as well
            match self.read_message() {
                Err(Error::FailedToReadMessage(e))
                    if e.kind() == std::io::ErrorKind::InvalidInput =>
                {
                    error!("Function did not write a FunctionResult or FatalError to its stdout before stopping");
                    return match self.wait_to_finish_and_get_usage() {
                        Ok(u) => Err((Error::FunctionDidntTerminateCleanly, u)),
                        Err((e, u)) => Err((e, u)),
                    };
                }
                Err(e) => {
                    error!("Could not receive message from instance: {e:?}");
                    return match self.wait_to_finish_and_get_usage() {
                        Ok(u) => Err((e, u)),
                        Err((e, u)) => Err((e, u)),
                    };
                }
                Ok(message) => {
                    trace!("Message from function {}: {:?}", self.id, message);
                    match message {
                        OutgoingMessage::FunctionResult(response) => {
                            return match self.wait_to_finish_and_get_usage() {
                                Ok(u) => Ok((response, u)),
                                Err((e, u)) => Err((e, u)),
                            };
                        }

                        OutgoingMessage::FatalError(e) => {
                            let e = Error::FunctionRuntimeError(FunctionRuntimeError::FatalError(
                                e.error.into_owned(),
                            ));
                            return match self.wait_to_finish_and_get_usage() {
                                Ok(u) => Err((e, u)),
                                Err((_, u)) => Err((e, u)),
                            };
                        }

                        OutgoingMessage::Log(log) => {
                            if self.include_logs {
                                let level = match log.level {
                                    LogLevel::Error => Level::Error,
                                    LogLevel::Warn => Level::Warn,
                                    LogLevel::Info => Level::Info,
                                    LogLevel::Debug => Level::Debug,
                                    LogLevel::Trace => Level::Trace,
                                };

                                log!(
                                    target: FUNCTION_LOG_TARGET,
                                    level,
                                    "{}: {}",
                                    self.id,
                                    log.body
                                );
                            }
                        } // OutgoingMessage::DatabaseRequest(request) => {
                          //     let resp = match request {
                          //         DatabaseRequest::CreateTable(req) => {
                          //             let res = tokio::runtime::Handle::current()
                          //                 .block_on(self.database_service.create_table(
                          //                     create_database_id(
                          //                         &self.id.function_id.stack_id,
                          //                         req.db_name.into_owned(),
                          //                     ),
                          //                     req.table_name.into_owned(),
                          //                 ))
                          //                 .map_err(|e| e.to_string())
                          //                 .map(|d| musdk_common::database::TableDescription {
                          //                     table_name: Cow::Owned(d.table_name),
                          //                 });

                          //             self.state.database_write_count += 1;

                          //             DatabaseResponse::CreateTable(res)
                          //         }

                          //         DatabaseRequest::DropTable(req) => {
                          //             let res = tokio::runtime::Handle::current()
                          //                 .block_on(self.database_service.delete_table(
                          //                     create_database_id(
                          //                         &self.id.function_id.stack_id,
                          //                         req.db_name.into_owned(),
                          //                     ),
                          //                     req.table_name.into_owned(),
                          //                 ))
                          //                 .map_err(|e| e.to_string())
                          //                 .map(|r| {
                          //                     r.map(|d| musdk_common::database::TableDescription {
                          //                         table_name: Cow::Owned(d.table_name),
                          //                     })
                          //                 });

                          //             self.state.database_write_count += 1;

                          //             DatabaseResponse::DropTable(res)
                          //         }

                          //         DatabaseRequest::Find(req) => {
                          //             let res = tokio::runtime::Handle::current()
                          //                 .block_on(self.database_service.find_item(
                          //                     create_database_id(
                          //                         &self.id.function_id.stack_id,
                          //                         req.db_name.into_owned(),
                          //                     ),
                          //                     req.table_name.into_owned(),
                          //                     key_filter_to_mudb(req.key_filter),
                          //                     req.value_filter.to_string().try_into().map_err(
                          //                         |_| {
                          //                             (
                          //                                 Error::DBError(
                          //                                     "failed to parse value filter",
                          //                                 ),
                          //                                 vec![],
                          //                             )
                          //                         },
                          //                     )?,
                          //                 ))
                          //                 .map_err(|e| e.to_string())
                          //                 .map(|r| {
                          //                     r.into_iter()
                          //                         .map(|(k, v)| musdk_common::database::Item {
                          //                             key: Cow::Owned(k),
                          //                             value: Cow::Owned(v),
                          //                         })
                          //                         .collect()
                          //                 });

                          //             self.state.database_read_count += 1;

                          //             DatabaseResponse::Find(res)
                          //         }
                          //         DatabaseRequest::Insert(req) => {
                          //             let res = tokio::runtime::Handle::current()
                          //                 .block_on({
                          //                     self.database_service.insert_one_item(
                          //                         create_database_id(
                          //                             &self.id.function_id.stack_id,
                          //                             req.db_name.into_owned(),
                          //                         ),
                          //                         req.table_name.into_owned(),
                          //                         req.key.into_owned(),
                          //                         req.value.into_owned(),
                          //                     )
                          //                 })
                          //                 .map_err(|e| e.to_string())
                          //                 .map(Cow::Owned);

                          //             self.state.database_write_count += 1;

                          //             DatabaseResponse::Insert(res)
                          //         }
                          //         DatabaseRequest::Update(req) => {
                          //             let res = tokio::runtime::Handle::current()
                          //                 .block_on(self.database_service.update_item(
                          //                     create_database_id(
                          //                         &self.id.function_id.stack_id,
                          //                         req.db_name.into_owned(),
                          //                     ),
                          //                     req.table_name.into_owned(),
                          //                     key_filter_to_mudb(req.key_filter),
                          //                     req.value_filter.to_string().try_into().map_err(
                          //                         |_| {
                          //                             (
                          //                                 Error::DBError(
                          //                                     "failed to parse value filter",
                          //                                 ),
                          //                                 vec![],
                          //                             )
                          //                         },
                          //                     )?,
                          //                     req.update.to_string().try_into().map_err(|_| {
                          //                         (Error::DBError("failed to parse updater"), vec![])
                          //                     })?,
                          //                 ))
                          //                 .map_err(|e| e.to_string())
                          //                 .map(|r| {
                          //                     r.into_iter()
                          //                         .map(|(k, v)| musdk_common::database::Item {
                          //                             key: Cow::Owned(k),
                          //                             value: Cow::Owned(v),
                          //                         })
                          //                         .collect()
                          //                 });

                          //             self.state.database_write_count += 1;

                          //             DatabaseResponse::Update(res)
                          //         }
                          //     };
                          //     self.write_message(IncomingMessage::DatabaseResponse(resp))
                          //         .map_err(|e| (e, vec![]))?;
                          // }
                    }
                }
            }
        }
    }
}

#[derive(Copy, Clone)]
enum IOState {
    Idle,
    Processing,
    // InRuntimeCall,
    // Closed,
}

mod utils {
    // use crate::mudb::service::DatabaseID;
    // use mu_stack::StackID;

    // pub fn create_database_id(stack_id: &StackID, db_name: String) -> DatabaseID {
    //     DatabaseID {
    //         stack_id: *stack_id,
    //         db_name,
    //     }
    // }

    // pub fn key_filter_to_mudb(
    //     key_filter: musdk_common::database::KeyFilter,
    // ) -> crate::mudb::service::KeyFilter {
    //     match key_filter {
    //         musdk_common::database::KeyFilter::Exact(k) => {
    //             crate::mudb::service::KeyFilter::Exact(k.into_owned())
    //         }
    //         musdk_common::database::KeyFilter::Prefix(k) => {
    //             crate::mudb::service::KeyFilter::Prefix(k.into_owned())
    //         }
    //     }
    // }
}
