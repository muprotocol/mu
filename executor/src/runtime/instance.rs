use std::{collections::HashMap, sync::Arc};

use super::{
    error::{Error, FunctionLoadingError},
    function,
    memory::create_memory,
    packet::{self, Packet},
    types::{FunctionHandle, FunctionID, InstanceID},
};
use crate::{
    mudb::service::DatabaseManager, runtime::error::FunctionRuntimeError,
    stack::usage_aggregator::Usage,
};

use anyhow::{anyhow, Result};
use borsh::{BorshDeserialize, BorshSerialize};
use log::{error, trace};
use wasmer::{CompilerConfig, Module, RuntimeError, Store};
use wasmer_compiler_llvm::LLVM;
use wasmer_middlewares::{metering::MeteringPoints, Metering};

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
) -> Vec<Usage> {
    let memory_megabytes = memory
        .get_adjusted_unit(byte_unit::ByteUnit::MB)
        .get_value();
    let memory_megabytes = memory_megabytes.floor() as u64;

    vec![
        Usage::DBRead {
            weak_reads: db_read,
            strong_reads: 0,
        },
        Usage::DBWrite {
            weak_writes: db_write,
            strong_writes: 0,
        },
        Usage::FunctionMBInstructions {
            memory_megabytes,
            instructions: instructions_count,
        },
    ]
}

fn metering_point_to_instructions_count(points: MeteringPoints) -> u64 {
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
}
impl InstanceState for Running {}

#[derive(Debug)]
pub struct Instance<S: InstanceState> {
    id: InstanceID,
    state: S,
    database_service: Arc<DatabaseManager>,
}

impl Instance<Loaded> {
    pub fn new(
        function_id: FunctionID,
        envs: HashMap<String, String>,
        store: Store,
        module: Module,
        database_service: Arc<DatabaseManager>,
    ) -> Self {
        let state = Loaded {
            store,
            envs,
            module,
        };

        Instance {
            id: InstanceID::generate_random(function_id),
            state,
            database_service,
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
        };
        Ok(Instance {
            id: self.id,
            state,
            database_service: self.database_service,
        })
    }
}

impl Instance<Running> {
    pub fn is_finished(&mut self) -> bool {
        self.state.handle.is_finished()
    }

    fn send_packet<'a>(&mut self, input: Packet<'a>) -> Result<(), Error> {
        trace!(
            "Sending packet to function {:?}, packet: {:?}",
            self.id,
            input
        );

        if let Err(e) = BorshSerialize::serialize(&input, &mut self.state.handle.io.stdin) {
            error!("failed to write data to function: {e}");

            return Err(Error::Internal(anyhow!(
                "failed to write data to function {e}",
            )));
        };

        // Do we need this?
        //self.state
        //    .handle
        //    .io
        //    .stdin
        //    .flush()
        //    .map_err(|e| Error::Internal(anyhow!("can not flush written message: {e}")))?;

        Ok(())
    }

    fn receive_packet<'a>(&'a mut self) -> Result<Packet<'a>, Error> {
        BorshDeserialize::deserialize_reader(&mut self.state.handle.io.stdout).map_err(|e| {
            error!("Error in deserializing packet: {e}");
            Error::Internal(anyhow!("failed to receive packet from function"))
        })
    }

    //TODO:
    // - The case when we are waiting for function's respond and function is exited is not covered.
    // and there should be a timeout for the amount of time we are waiting for function response.
    //
    // - It is not good to pass memory_limit here, but will do for now to be able to make usages all
    // here and encapsulate the usage making process.
    pub async fn run_request(
        self,
        memory_limit: byte_unit::Byte,
        request: Packet<'static>,
    ) -> Result<(packet::gateway::Response, Vec<Usage>), (Error, Vec<Usage>)> {
        tokio::task::spawn_blocking(move || self.inner_run_request(memory_limit, request))
            .await
            .map_err(|_| {
                (
                    Error::Internal(anyhow!("can not run function task to end")),
                    vec![],
                )
            })?
    }

    #[inline]
    fn inner_run_request<'a>(
        mut self,
        memory_limit: byte_unit::Byte,
        request: Packet<'static>,
    ) -> Result<(packet::gateway::Response, Vec<Usage>), (Error, Vec<Usage>)> {
        //TODO: Refactor these to `week` and `strong` when we had database replication
        let (mut database_read_count, mut database_write_count) = (0, 0);

        trace!(
            "Running function `{}` instance `{}`",
            self.id.function_id,
            self.id.instance_id
        );

        if self.is_finished() {
            trace!(
                "Instance {:?} is already exited before sending request",
                self.id
            );

            return Err((
                Error::FunctionRuntimeError(FunctionRuntimeError::FunctionEarlyExit(
                    RuntimeError::new("Function Early Exit"),
                )),
                vec![],
            ));
        }

        self.send_packet(request).map_err(|e| (e, vec![]))?;
        self.state.io_state = IOState::Processing;

        loop {
            match (self.state.io_state, self.is_finished()) {
                // Should never get into this case
                (IOState::Idle, _) => {
                    return Err((
                        Error::Internal(anyhow!(
                            "Invalid instance io state, should be processing, was idle"
                        )),
                        vec![],
                    ));
                }
                (IOState::Processing, true) => {
                    trace!(
                        "Instance {:?} is finished while still processing request",
                        self.id
                    );
                    return Err((
                        Error::FunctionRuntimeError(FunctionRuntimeError::FunctionEarlyExit(
                            RuntimeError::new("Function Early Exit"),
                        )),
                        vec![],
                    ));
                }
                (IOState::Processing, false) => todo!(),
                (IOState::InRuntimeCall, true) => todo!(),
                (IOState::InRuntimeCall, false) => todo!(),
                (IOState::Closed, true) => todo!(),
                (IOState::Closed, false) => todo!(),
            };
        }

        //match self.recieve_packet() {
        //    Ok(message) => match message.r#type.as_str() {
        //        GatewayResponse::TYPE => {
        //            let resp = GatewayResponse::from_message(message).map_err(|e| (e, vec![]))?;

        //            let result = tokio::runtime::Handle::current()
        //                .block_on(self.state.handle.join_handle)
        //                .map(move |m| {
        //                    m.map(|points| {
        //                        (
        //                            resp,
        //                            create_usage(
        //                                database_read_count,
        //                                database_write_count,
        //                                metering_point_to_instructions_count(points),
        //                                memory_limit,
        //                            ),
        //                        )
        //                    })
        //                    .map_err(|(e, points)| {
        //                        (
        //                            e,
        //                            create_usage(
        //                                database_read_count,
        //                                database_write_count,
        //                                metering_point_to_instructions_count(points),
        //                                memory_limit,
        //                            ),
        //                        )
        //                    })
        //                })
        //                .map_err(|_| {
        //                    (
        //                        Error::Internal(anyhow!("Failed to run function task to end")),
        //                        vec![],
        //                    )
        //                })?;

        //            return result;
        //        }

        //        DbRequest::TYPE => {
        //            let db_req = DbRequest::from_message(message).map_err(|e| (e, vec![]))?;
        //            let db_resp = match db_req.request {
        //                DbRequestDetails::CreateTable(req) => {
        //                    let res = tokio::runtime::Handle::current()
        //                        .block_on(self.database_service.create_table(
        //                            database_id(&self.id.function_id, req.db_name),
        //                            req.table_name,
        //                        ))
        //                        .map_err(|e| e.to_string());

        //                    database_write_count += 1;

        //                    DbResponse {
        //                        id: db_req.id,
        //                        response: DbResponseDetails::CreateTable(res),
        //                    }
        //                }

        //                DbRequestDetails::DropTable(req) => {
        //                    let res = tokio::runtime::Handle::current()
        //                        .block_on(self.database_service.delete_table(
        //                            database_id(&self.id.function_id, req.db_name),
        //                            req.table_name,
        //                        ))
        //                        .map_err(|e| e.to_string());

        //                    database_write_count += 1;

        //                    DbResponse {
        //                        id: db_req.id,
        //                        response: DbResponseDetails::DropTable(res),
        //                    }
        //                }

        //                DbRequestDetails::Find(req) => {
        //                    let res = tokio::runtime::Handle::current()
        //                        .block_on(self.database_service.find_item(
        //                            database_id(&self.id.function_id, req.db_name),
        //                            req.table_name,
        //                            req.key_filter,
        //                            req.value_filter.try_into().map_err(|_| {
        //                                (Error::DBError("failed to parse value filter"), vec![])
        //                            })?,
        //                        ))
        //                        .map_err(|e| e.to_string());

        //                    database_read_count += 1;

        //                    DbResponse {
        //                        id: db_req.id,
        //                        response: DbResponseDetails::Find(res),
        //                    }
        //                }
        //                DbRequestDetails::Insert(req) => {
        //                    let res = tokio::runtime::Handle::current()
        //                        .block_on({
        //                            self.database_service.insert_one_item(
        //                                database_id(&self.id.function_id, req.db_name),
        //                                req.table_name,
        //                                req.key,
        //                                req.value,
        //                            )
        //                        })
        //                        .map_err(|e| e.to_string());

        //                    database_write_count += 1;

        //                    DbResponse {
        //                        id: db_req.id,
        //                        response: DbResponseDetails::Insert(res),
        //                    }
        //                }
        //                DbRequestDetails::Update(req) => {
        //                    let res = tokio::runtime::Handle::current()
        //                        .block_on(self.database_service.update_item(
        //                            database_id(&self.id.function_id, req.db_name),
        //                            req.table_name,
        //                            req.key_filter,
        //                            req.value_filter.try_into().map_err(|_| {
        //                                (Error::DBError("failed to parse value filter"), vec![])
        //                            })?,
        //                            req.update.try_into().map_err(|_| {
        //                                (Error::DBError("failed to parse updater"), vec![])
        //                            })?,
        //                        ))
        //                        .map_err(|e| e.to_string());

        //                    database_write_count += 1;

        //                    DbResponse {
        //                        id: db_req.id,
        //                        response: DbResponseDetails::Update(res),
        //                    }
        //                }
        //            };

        //            let msg = db_resp.to_message().map_err(|e| (e, vec![]))?;
        //            self.send_packet(msg).map_err(|e| (e, vec![]))?;
        //        }

        //        Log::TYPE => {
        //            let log = Log::from_message(message).map_err(|e| (e, vec![]))?;
        //            println!("Log: {log:#?}");
        //        }
        //        t => return Err((Error::InvalidMessageType(t.to_string()), vec![])),
        //    },
        //    Err(e) => println!("Error while parsing response: {e:?}"),
        //};
    }
}

#[derive(Copy, Clone)]
enum IOState {
    Idle,
    Processing,
    InRuntimeCall,
    Closed,
}
