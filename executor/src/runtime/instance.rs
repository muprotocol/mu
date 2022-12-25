use std::{collections::HashMap, sync::Arc};

use super::{
    error::{Error, FunctionLoadingError},
    function,
    memory::create_memory,
    packet::{self, IntoPacket, Packet},
    types::{FunctionHandle, FunctionID, InstanceID},
};
use crate::{
    mudb::service::DatabaseManager,
    runtime::{
        error::FunctionRuntimeError,
        packet::{database::database_id, FromPacket, PacketType},
    },
    stack::usage_aggregator::Usage,
};

use anyhow::anyhow;
use borsh::{BorshDeserialize, BorshSerialize};
use log::{error, info, trace};
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
    db_read: &u64,
    db_write: &u64,
    instructions_count: u64,
    memory: &byte_unit::Byte,
) -> Vec<Usage> {
    let memory_megabytes = memory
        .get_adjusted_unit(byte_unit::ByteUnit::MB)
        .get_value();
    let memory_megabytes = memory_megabytes.floor() as u64;

    vec![
        Usage::DBRead {
            weak_reads: *db_read,
            strong_reads: 0,
        },
        Usage::DBWrite {
            weak_writes: *db_write,
            strong_writes: 0,
        },
        Usage::FunctionMBInstructions {
            memory_megabytes,
            instructions: instructions_count,
        },
    ]
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

    next_message_id: u64,

    //TODO: Refactor these to `week` and `strong` when we had database replication
    database_write_count: u64,
    database_read_count: u64,
}

impl InstanceState for Running {}

#[derive(Debug)]
pub struct Instance<S: InstanceState> {
    id: InstanceID,
    state: S,
    database_service: Arc<DatabaseManager>,
    memory_limit: byte_unit::Byte,
}

impl Instance<Loaded> {
    pub fn new(
        function_id: FunctionID,
        envs: HashMap<String, String>,
        store: Store,
        module: Module,
        database_service: Arc<DatabaseManager>,
        memory_limit: byte_unit::Byte,
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
            memory_limit,
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
            next_message_id: 1, // 0 was the first message (request)
            database_write_count: 0,
            database_read_count: 0,
        };
        Ok(Instance {
            id: self.id,
            state,
            database_service: self.database_service,
            memory_limit: self.memory_limit,
        })
    }
}

impl Instance<Running> {
    pub fn is_finished(&mut self) -> bool {
        self.state.handle.is_finished()
    }

    fn send_packet<'a>(&mut self, packet: &Packet<'a>) -> Result<(), Error> {
        trace!(
            "Sending packet to function {:?}, packet: {:?}",
            self.id,
            packet
        );

        if let Err(e) = BorshSerialize::serialize(packet, &mut self.state.handle.io.stdin) {
            error!("failed to write data to function: {e}");

            return Err(Error::Internal(anyhow!(
                "failed to write data to function {e}",
            )));
        };

        //TODO: Do we need this?
        //self.state
        //    .handle
        //    .io
        //    .stdin
        //    .flush()
        //    .map_err(|e| Error::Internal(anyhow!("can not flush written message: {e}")))?;

        Ok(())
    }

    fn send_raw_packet<'a, P: IntoPacket<'a>>(&mut self, input: &'a P) -> Result<(), Error> {
        let packet = input
            .into_packet(self.state.next_message_id)
            .map_err(|e| Error::FunctionRuntimeError(FunctionRuntimeError::SerializtionError(e)))?;

        self.send_packet(&packet)?;
        self.state.next_message_id += 1;

        Ok(())
    }

    fn receive_packet<'a>(&'a mut self) -> Result<Packet<'a>, Error> {
        BorshDeserialize::deserialize_reader(&mut self.state.handle.io.stdout).map_err(|e| {
            error!("Error in deserializing packet: {e}");
            Error::Internal(anyhow!("failed to receive packet from function"))
        })
    }

    #[inline]
    pub async fn run_request(
        self,
        request: Packet<'static>,
    ) -> Result<(packet::gateway::Response, Vec<Usage>), (Error, Vec<Usage>)> {
        tokio::task::spawn_blocking(move || self.inner_run_request(request))
            .await
            .map_err(|_| {
                (
                    Error::Internal(anyhow!("can not run function task to end")),
                    vec![],
                )
            })?
    }

    fn inner_run_request<'a>(
        mut self,
        request: Packet<'static>,
    ) -> Result<(packet::gateway::Response, Vec<Usage>), (Error, Vec<Usage>)> {
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
                vec![],
            ));
        }

        self.send_packet(&request).map_err(|e| (e, vec![]))?;
        self.state.io_state = IOState::Processing;

        loop {
            // Check function state
            match (self.state.io_state, self.is_finished()) {
                // Should never get into this case
                (IOState::Idle, _) => {
                    trace!(
                        "Instance {:?} is in invalid io-state, should be processing, was idle",
                        &self.id
                    );
                    return Err((
                        Error::Internal(anyhow!(
                            "Invalid instance io-state, should be processing, was idle"
                        )),
                        vec![],
                    ));
                }
                (IOState::Processing, true) => {
                    trace!(
                        "Instance {:?} exited while was in processing state",
                        &self.id
                    );
                    return Err((
                        Error::FunctionRuntimeError(FunctionRuntimeError::FunctionEarlyExit(
                            RuntimeError::new("Function Early Exit"),
                        )),
                        vec![],
                    ));
                }
                (IOState::Processing, false) => (),
                (IOState::InRuntimeCall, true) => {
                    trace!(
                        "Instance {:?} exited while was in runtime-call state",
                        &self.id
                    );
                    return Err((
                        Error::FunctionRuntimeError(FunctionRuntimeError::FunctionEarlyExit(
                            RuntimeError::new("Function Early Exit"),
                        )),
                        vec![],
                    ));
                }
                (IOState::InRuntimeCall, false) => (),
                (IOState::Closed, true) => (),
                (IOState::Closed, false) => {
                    trace!("Instance {:?} has io closed but still running", &self.id);
                    return Err((
                        Error::FunctionRuntimeError(FunctionRuntimeError::FunctionEarlyExit(
                            RuntimeError::new("IO Closed"),
                        )),
                        vec![],
                    ));
                }
            }

            // Now process the Runtime calls from function
            match self.receive_packet() {
                Err(e) => {
                    //TODO: Handle better
                    error!("can not receive packet from instance, {e}");
                    ()
                }
                Ok(packet) => {
                    match packet.data_type() {
                        PacketType::GatewayResponse => {
                            let resp =
                                packet::gateway::Response::from_packet(packet).map_err(|e| {
                                    Error::FunctionRuntimeError(
                                        FunctionRuntimeError::SerializtionError(e),
                                    )
                                });

                            let result = tokio::runtime::Handle::current()
                            .block_on(self.state.handle.join_handle)
                            .map(move |metering_points| {
                                let usage = |m| {
                                    create_usage(
                                        &self.state.database_read_count,
                                        &self.state.database_write_count,
                                        metering_point_to_instructions_count(m),
                                        &self.memory_limit,
                                    )
                                };

                                match (metering_points, resp) {
                                    (Ok(m), Ok(r)) => Ok((r, usage(&m))),
                                    (Ok(m), Err(e)) => Err((e, usage(&m))),
                                    (Err((error, m)), Ok(r)) => {
                                        error!( "function ran into error but produced the response, {error}");
                                        Ok((r, usage(&m)))
                                    }
                                    (Err((error, m)), Err(r)) => {
                                        error!("function ran into error but produced the response, {error}");
                                        Err((r, usage(&m)))
                                    }
                                }
                            })
                            .map_err(|_| {
                                (
                                    Error::Internal(anyhow!("Failed to run function task to end")),
                                    vec![],
                                )
                            })?;

                            return result;
                        }
                        PacketType::Log => match packet::log::Log::from_packet(packet) {
                            //TODO: Log into a log service
                            Ok(log) => info!("[Log] [Instance-{}]: {}", &self.id, log),
                            Err(e) => error!("can not deserialize packet into {}, {}", "Log", e),
                        },
                        PacketType::DbRequest => todo!(),
                        PacketType::DbRequest => {
                            match packet::database::Request::from_packet(packet) {
                                Ok(req) => {
                                    let resp = match req {
                                        packet::database::Request::CreateTable(req) => {
                                            let res = tokio::runtime::Handle::current()
                                                .block_on(self.database_service.create_table(
                                                    database_id(&self.id.function_id, req.db_name),
                                                    req.table_name,
                                                ))
                                                .map_err(|e| e.to_string());

                                            self.state.database_write_count += 1;

                                            packet::database::Response::CreateTable(res)
                                        }

                                        packet::database::Request::DropTable(req) => {
                                            let res = tokio::runtime::Handle::current()
                                                .block_on(self.database_service.delete_table(
                                                    database_id(&self.id.function_id, req.db_name),
                                                    req.table_name,
                                                ))
                                                .map_err(|e| e.to_string());

                                            self.state.database_write_count += 1;

                                            packet::database::Response::DropTable(res)
                                        }

                                        packet::database::Request::Find(req) => {
                                            let res = tokio::runtime::Handle::current()
                                                .block_on(self.database_service.find_item(
                                                    database_id(&self.id.function_id, req.db_name),
                                                    req.table_name,
                                                    req.key_filter,
                                                    req.value_filter.try_into().map_err(|_| {
                                                        (
                                                            Error::DBError(
                                                                "failed to parse value filter",
                                                            ),
                                                            vec![],
                                                        )
                                                    })?,
                                                ))
                                                .map_err(|e| e.to_string());

                                            self.state.database_read_count += 1;

                                            packet::database::Response::Find(res)
                                        }
                                        packet::database::Request::Insert(req) => {
                                            let res = tokio::runtime::Handle::current()
                                                .block_on({
                                                    self.database_service.insert_one_item(
                                                        database_id(
                                                            &self.id.function_id,
                                                            req.db_name,
                                                        ),
                                                        req.table_name,
                                                        req.key,
                                                        req.value,
                                                    )
                                                })
                                                .map_err(|e| e.to_string());

                                            self.state.database_write_count += 1;

                                            packet::database::Response::Insert(res)
                                        }
                                        packet::database::Request::Update(req) => {
                                            let res = tokio::runtime::Handle::current()
                                                .block_on(self.database_service.update_item(
                                                    database_id(&self.id.function_id, req.db_name),
                                                    req.table_name,
                                                    req.key_filter,
                                                    req.value_filter.try_into().map_err(|_| {
                                                        (
                                                            Error::DBError(
                                                                "failed to parse value filter",
                                                            ),
                                                            vec![],
                                                        )
                                                    })?,
                                                    req.update.try_into().map_err(|_| {
                                                        (
                                                            Error::DBError(
                                                                "failed to parse updater",
                                                            ),
                                                            vec![],
                                                        )
                                                    })?,
                                                ))
                                                .map_err(|e| e.to_string());

                                            self.state.database_write_count += 1;

                                            packet::database::Response::Update(res)
                                        }
                                    };

                                    //TODO: abort function or just log the error?
                                    let _ = self.send_raw_packet(&resp);
                                }
                                Err(e) => {
                                    error!("can not deserialize packet into {}, {}", "DbRequest", e)
                                }
                            }
                        }

                        // Should not get these packets from a function
                        PacketType::GatewayRequest | PacketType::DbResponse => {
                            error!("got invalid packet, ignoring")
                        }
                    }
                }
            };
        }
    }
}

#[derive(Copy, Clone)]
enum IOState {
    Idle,
    Processing,
    InRuntimeCall,
    Closed,
}
