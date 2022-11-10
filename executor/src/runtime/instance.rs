use std::{
    collections::HashMap,
    io::{BufRead, Write},
    sync::Arc,
};

use super::{
    error::Error,
    function,
    memory::create_memory,
    message::{database::*, gateway::*, log::Log, FromMessage, Message, ToMessage},
    types::{FunctionHandle, FunctionID, InstanceID},
};
use crate::{mudb::service::DatabaseManager, stack::usage_aggregator::Usage};

use anyhow::{anyhow, bail, Result};
use bytes::BufMut;
use log::trace;
use mu_stack::MegaByte;
use wasmer::{CompilerConfig, Module, Store};
use wasmer_compiler_llvm::LLVM;
use wasmer_middlewares::{metering::MeteringPoints, Metering};

const MESSAGE_READ_BUF_CAP: usize = 8 * 1024;

pub fn create_store(memory_limit: MegaByte) -> Store {
    let mut compiler_config = LLVM::default();

    let metering = Arc::new(Metering::new(u64::MAX, |_| 1));
    compiler_config.push_middleware(metering);

    let memory = create_memory(memory_limit);

    Store::new_with_tunables(compiler_config, memory)
}

fn create_usage(
    db_read: u64,
    db_write: u64,
    instructions_count: u64,
    memory: MegaByte,
) -> Vec<Usage> {
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
            memory_megabytes: memory.0 as u64,
            instructions: instructions_count,
        },
    ]
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

    pub fn start(self) -> Result<Instance<Running>> {
        let handle = function::start(self.state.store, &self.state.module, self.state.envs)?;
        let state = Running { handle };
        Ok(Instance {
            id: self.id,
            state,
            database_service: self.database_service,
        })
    }
}

impl Instance<Running> {
    pub fn is_finished(&mut self) -> bool {
        let is_finished = self.state.handle.is_finished();
        trace!(
            "Instance {:?} status is {}",
            self.id,
            if is_finished {
                "finished"
            } else {
                "still running "
            }
        );

        is_finished
    }

    fn write_to_stdin(&mut self, input: Message) -> Result<()> {
        let mut bytes = input.as_bytes()?;
        bytes.put_u8(b'\n');
        self.state.handle.io.stdin.write_all(&bytes)?;
        self.state.handle.io.stdin.flush()?;
        Ok(())
    }

    fn read_from_stdout(&mut self) -> Result<Message> {
        let mut buf = String::with_capacity(MESSAGE_READ_BUF_CAP);
        loop {
            let bytes_read = self.state.handle.io.stdout.read_line(&mut buf)?;
            if bytes_read == 0 {
                continue;
            };

            return serde_json::from_slice(buf.as_bytes()).map_err(Into::into);
        }
    }

    //TODO:
    // - The case when we are waiting for function's respond and function is exited is not covered.
    // and there should be a timeout for the amount of time we are waiting for function response.
    //
    // - It is not good to pass memory_limit here, but will do for now to be able to make usages all
    // here and encapsulate the usage making process.
    pub async fn run_request(
        self,
        memory_limit: MegaByte,
        request: Message,
    ) -> Result<(GatewayResponse, Vec<Usage>)> {
        tokio::task::spawn_blocking(move || self._run_request(memory_limit, request))
            .await
            .map_err(|_| anyhow!("can not run function task to end"))?
    }

    fn _run_request(
        mut self,
        memory_limit: MegaByte,
        request: Message,
    ) -> Result<(GatewayResponse, Vec<Usage>)> {
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
            return Err(Error::FunctionEarlyExit(self.id)).map_err(Into::into);
        }

        self.write_to_stdin(request)?;
        loop {
            if self.is_finished() {
                trace!("Instance {:?} exited early", self.id);
                return Err(Error::FunctionEarlyExit(self.id)).map_err(Into::into);
            }

            match self.read_from_stdout() {
                Ok(message) => match message.r#type.as_str() {
                    GatewayResponse::TYPE => {
                        let resp = GatewayResponse::from_message(message)?;

                        let usages = tokio::runtime::Handle::current()
                            .block_on(self.state.handle.join_handle)
                            .map(|m| {
                                let instructions_count = match m {
                                    MeteringPoints::Exhausted => u64::MAX,
                                    MeteringPoints::Remaining(p) => u64::MAX - p,
                                };

                                create_usage(
                                    database_read_count,
                                    database_write_count,
                                    instructions_count,
                                    memory_limit,
                                )
                            })
                            .map_err(|_| anyhow!("can not run function task to end"))?;

                        return Ok((resp, usages));
                    }

                    DbRequest::TYPE => {
                        let db_req = DbRequest::from_message(message)?;
                        let db_resp = match db_req.request {
                            DbRequestDetails::CreateTable(req) => {
                                let res = tokio::runtime::Handle::current()
                                    .block_on(self.database_service.create_table(
                                        database_id(&self.id.function_id, req.db_name),
                                        req.table_name,
                                    ))
                                    .map_err(|e| e.to_string());

                                database_write_count += 1;

                                DbResponse {
                                    id: db_req.id,
                                    response: DbResponseDetails::CreateTable(res),
                                }
                            }

                            DbRequestDetails::DropTable(req) => {
                                let res = tokio::runtime::Handle::current()
                                    .block_on(self.database_service.delete_table(
                                        database_id(&self.id.function_id, req.db_name),
                                        req.table_name,
                                    ))
                                    .map_err(|e| e.to_string());

                                database_write_count += 1;

                                DbResponse {
                                    id: db_req.id,
                                    response: DbResponseDetails::DropTable(res),
                                }
                            }

                            DbRequestDetails::Find(req) => {
                                let res = tokio::runtime::Handle::current()
                                    .block_on(self.database_service.find_item(
                                        database_id(&self.id.function_id, req.db_name),
                                        req.table_name,
                                        req.key_filter,
                                        req.value_filter.try_into()?,
                                    ))
                                    .map_err(|e| e.to_string());

                                database_read_count += 1;

                                DbResponse {
                                    id: db_req.id,
                                    response: DbResponseDetails::Find(res),
                                }
                            }
                            DbRequestDetails::Insert(req) => {
                                let res = tokio::runtime::Handle::current()
                                    .block_on({
                                        self.database_service.insert_one_item(
                                            database_id(&self.id.function_id, req.db_name),
                                            req.table_name,
                                            req.key,
                                            req.value,
                                        )
                                    })
                                    .map_err(|e| e.to_string());

                                database_write_count += 1;

                                DbResponse {
                                    id: db_req.id,
                                    response: DbResponseDetails::Insert(res),
                                }
                            }
                            DbRequestDetails::Update(req) => {
                                let res = tokio::runtime::Handle::current()
                                    .block_on(self.database_service.update_item(
                                        database_id(&self.id.function_id, req.db_name),
                                        req.table_name,
                                        req.key_filter,
                                        req.value_filter.try_into()?,
                                        req.update.try_into()?,
                                    ))
                                    .map_err(|e| e.to_string());

                                database_write_count += 1;

                                DbResponse {
                                    id: db_req.id,
                                    response: DbResponseDetails::Update(res),
                                }
                            }
                        };

                        let msg = db_resp.to_message()?;
                        self.write_to_stdin(msg)?;
                    }

                    Log::TYPE => {
                        let log = Log::from_message(message)?;
                        println!("Log: {log:#?}");
                    }
                    t => bail!("invalid message type: {t}"),
                },
                Err(e) => println!("Error while parsing response: {e:?}"),
            };
        }
    }
}
