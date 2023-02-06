use std::{borrow::Cow, collections::HashMap, future::Future, sync::Arc};

use super::{
    error::{Error, FunctionLoadingError},
    function,
    memory::create_memory,
    types::{ExecuteFunctionRequest, ExecuteFunctionResponse, FunctionHandle, InstanceID},
};
use crate::{error::FunctionRuntimeError, Usage};

use anyhow::anyhow;
use log::{error, log, trace, Level};
use mu_db::{error::Result as MudbResult, DbClient, DbManager, Key as MudbKey, Scan as MudbScan};
use mu_stack::{AssemblyID, StackID};
use musdk_common::{
    incoming_message::{
        db::{CasResult, DbError, EmptyResult, KvPair, KvPairsResult, ListResult, SingleResult},
        IncomingMessage,
    },
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

pub struct Instance<S: InstanceState> {
    id: InstanceID,
    state: S,
    memory_limit: byte_unit::Byte,
    include_logs: bool,
    db_manager: Box<dyn DbManager>,
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
        db_manager: Box<dyn DbManager>,
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
            db_manager,
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
            db_manager: self.db_manager,
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

        // TODO: debug
        trace!("going 1");

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
                        }

                        OutgoingMessage::Put(req) => {
                            self.db_request(|db_client, stack_id| async move {
                                let key = make_mu_db_key(stack_id, req.table, req.key)?;
                                db_client
                                    .put(key, req.value.into_owned(), req.is_atomic)
                                    .await
                                    .map(tuple_type_to_incoming_msg)
                            })?
                        }

                        OutgoingMessage::Get(req) => {
                            self.db_request(|db_client, stack_id| async move {
                                let key = make_mu_db_key(stack_id, req.table, req.key)?;
                                db_client.get(key).await.map(option_blob_to_incoming_msg)
                            })?
                        }

                        OutgoingMessage::Delete(req) => {
                            self.db_request(|db_client, stack_id| async move {
                                let key = make_mu_db_key(stack_id, req.table, req.key)?;
                                db_client
                                    .delete(key, req.is_atomic)
                                    .await
                                    .map(tuple_type_to_incoming_msg)
                            })?
                        }

                        OutgoingMessage::DeleteByPrefix(req) => {
                            self.db_request(|db_client, stack_id| async move {
                                db_client
                                    .delete_by_prefix(
                                        stack_id,
                                        req.table.into_owned().try_into()?,
                                        req.key_prefix.into_owned(),
                                    )
                                    .await
                                    .map(tuple_type_to_incoming_msg)
                            })?
                        }

                        OutgoingMessage::Scan(req) => {
                            self.db_request(|db_client, stack_id| async move {
                                let db_key = make_mu_db_scan(stack_id, req.table, req.key_prefix)?;
                                db_client
                                    .scan(db_key, req.limit)
                                    .await
                                    .map(kv_pairs_to_incoming_msg)
                            })?
                        }

                        OutgoingMessage::ScanKeys(req) => {
                            self.db_request(|db_client, stack_id| async move {
                                let db_key = make_mu_db_scan(stack_id, req.table, req.key_prefix)?;
                                db_client
                                    .scan_keys(db_key, req.limit)
                                    .await
                                    .map(list_to_incoming_msg)
                            })?
                        }

                        OutgoingMessage::BatchPut(req) => {
                            self.db_request(|db_client, stack_id| async move {
                                db_client
                                    .batch_put(
                                        req.table_key_value_triples
                                            .into_iter()
                                            .map(|(table, key, value)| {
                                                make_mu_db_key(stack_id, table, key)
                                                    .map(|db_key| (db_key, value.into_owned()))
                                            })
                                            .collect::<MudbResult<_>>()?,
                                        req.is_atomic,
                                    )
                                    .await
                                    .map(tuple_type_to_incoming_msg)
                            })?
                        }

                        OutgoingMessage::BatchGet(req) => {
                            self.db_request(|db_client, stack_id| async move {
                                let keys = make_mu_db_keys(stack_id, req.table_key_tuples)?;
                                db_client
                                    .batch_get(keys)
                                    .await
                                    .map(kv_pairs_to_incoming_msg)
                            })?
                        }

                        OutgoingMessage::BatchDelete(req) => {
                            self.db_request(|db_client, stack_id| async move {
                                let keys = make_mu_db_keys(stack_id, req.table_key_tuples)?;
                                db_client
                                    .batch_delete(keys)
                                    .await
                                    .map(tuple_type_to_incoming_msg)
                            })?
                        }

                        OutgoingMessage::BatchScan(req) => {
                            self.db_request(|db_client, stack_id| async move {
                                let scans =
                                    make_mu_db_scans(stack_id, req.table_key_prefixe_tuples)?;
                                db_client
                                    .batch_scan(scans, req.each_limit)
                                    .await
                                    .map(kv_pairs_to_incoming_msg)
                            })?
                        }

                        OutgoingMessage::BatchScanKeys(req) => {
                            self.db_request(|db_client, stack_id| async move {
                                let scans =
                                    make_mu_db_scans(stack_id, req.table_key_prefixe_tuples)?;
                                db_client
                                    .batch_scan_keys(scans, req.each_limit)
                                    .await
                                    .map(list_to_incoming_msg)
                            })?
                        }

                        OutgoingMessage::TableList(req) => {
                            panic!("before talbelist request");
                            self.db_request(|db_client, stack_id| async move {
                                db_client
                                    .table_list(
                                        stack_id,
                                        Some(req.table_prefix.into_owned().try_into()?),
                                    )
                                    .await
                                    .map(list_to_incoming_msg)
                            })?;
                        }

                        OutgoingMessage::CompareAndSwap(req) => {
                            self.db_request(|db_client, stack_id| async move {
                                let key = make_mu_db_key(stack_id, req.table, req.key)?;
                                db_client
                                    .compare_and_swap(
                                        key,
                                        Some(req.previous_value.into_owned()),
                                        req.new_value.into_owned(),
                                    )
                                    .await
                                    .map(|(x, is_swapped)| {
                                        let y = if is_swapped { x.unwrap() } else { vec![] };
                                        IncomingMessage::CasResult(CasResult {
                                            previous_value: Cow::Owned(y),
                                            is_swapped,
                                        })
                                    })
                            })?
                        }
                    }
                }
            }
        }
    }

    fn db_request<'a, A, B>(&mut self, f: A) -> Result<(), (Error, Usage)>
    where
        A: FnOnce(Box<dyn DbClient>, StackID) -> B,
        B: Future<Output = MudbResult<IncomingMessage<'a>>>,
    {
        tokio::runtime::Handle::current().block_on(async {
            let stack_id = self.id.function_id.stack_id;
            match self.db_manager.make_client().await {
                Ok(db_client) => {
                    let msg = f(db_client, stack_id).await.unwrap_or_else(|e| {
                        IncomingMessage::DbError(DbError {
                            error: Cow::from(format!("{e:?}")),
                        })
                    });
                    self.write_message(msg)
                }
                Err(e) => self.write_message(IncomingMessage::DbError(DbError {
                    error: Cow::from(format!("{e:?}")),
                })),
            }
            .map_err(|e| (e, Usage::default()))
        })
    }
}

fn make_mu_db_key(
    stack_id: StackID,
    cow_table: Cow<'_, [u8]>,
    cow_key: Cow<'_, [u8]>,
) -> MudbResult<MudbKey> {
    Ok(MudbKey {
        stack_id,
        table_name: cow_table.into_owned().try_into()?,
        inner_key: cow_key.into_owned(),
    })
}

fn make_mu_db_scan(
    stack_id: StackID,
    cow_table: Cow<'_, [u8]>,
    cow_key_prefix: Cow<'_, [u8]>,
) -> MudbResult<MudbScan> {
    Ok(MudbScan::ByInnerKeyPrefix(
        stack_id,
        cow_table.into_owned().try_into()?,
        cow_key_prefix.into_owned(),
    ))
}

type TableKeyPairs<'a> = Vec<(Cow<'a, [u8]>, Cow<'a, [u8]>)>;

fn make_mu_db_keys(
    stack_id: StackID,
    table_key_list: TableKeyPairs<'_>,
) -> MudbResult<Vec<MudbKey>> {
    table_key_list
        .into_iter()
        .map(|(table, key)| make_mu_db_key(stack_id, table, key))
        .collect::<MudbResult<_>>()
}

fn make_mu_db_scans(
    stack_id: StackID,
    table_key_list: TableKeyPairs<'_>,
) -> MudbResult<Vec<MudbScan>> {
    table_key_list
        .into_iter()
        .map(|(table, key)| make_mu_db_scan(stack_id, table, key))
        .collect::<MudbResult<_>>()
}

#[derive(Copy, Clone)]
enum IOState {
    Idle,
    Processing,
    // InRuntimeCall,
    // Closed,
}

fn option_blob_to_incoming_msg<'a>(x: Option<Vec<u8>>) -> IncomingMessage<'a> {
    match x {
        Some(xp) => IncomingMessage::SingleResult(SingleResult {
            item: Cow::Owned(xp),
        }),
        None => IncomingMessage::EmptyResult(EmptyResult),
    }
}

fn tuple_type_to_incoming_msg<'a>(_: ()) -> IncomingMessage<'a> {
    IncomingMessage::EmptyResult(EmptyResult)
}

fn kv_pairs_to_incoming_msg<'a>(x: Vec<(MudbKey, Vec<u8>)>) -> IncomingMessage<'a> {
    IncomingMessage::KvPairsResult(KvPairsResult {
        kv_pairs: x
            .into_iter()
            .map(|(k, v)| KvPair {
                key: Cow::Owned(Vec::from(k)),
                value: Cow::Owned(v),
            })
            .collect(),
    })
}

fn list_to_incoming_msg<'a, T>(x: Vec<T>) -> IncomingMessage<'a>
where
    Vec<u8>: From<T>,
{
    IncomingMessage::ListResult(ListResult {
        items: x.into_iter().map(Vec::<u8>::from).map(Cow::Owned).collect(),
    })
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
