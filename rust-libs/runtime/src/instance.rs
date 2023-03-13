mod database;
mod http_client;
pub(crate) mod utils;

use std::{borrow::BorrowMut, ops::Deref};
use std::{borrow::Cow, collections::HashMap, future::Future};

use crate::{
    error::{Error, FunctionRuntimeError, Result},
    function,
    instance::utils::create_usage,
    types::{ExecuteFunctionRequest, ExecuteFunctionResponse, FunctionHandle, InstanceID},
    Usage,
};

use mu_db::{DbClient, DbManager};
use mu_stack::StackID;
use mu_storage::{StorageClient, StorageManager};
use musdk_common::{
    incoming_message::{
        self,
        db::*,
        storage::{ObjectListResult, StorageEmptyResult, StorageError, StorageGetResult},
        IncomingMessage,
    },
    outgoing_message::{LogLevel, OutgoingMessage},
};

use anyhow::anyhow;
use log::{error, log, trace, Level};
use wasmer::{Module, Store};

const FUNCTION_LOG_TARGET: &str = "mu_function";

type ResultWithUsage<T> = Result<T, (Error, Usage)>;

pub(crate) struct Instance {
    id: InstanceID,
    handle: FunctionHandle,

    // Options
    memory_limit: byte_unit::Byte,
    include_logs: bool,

    // Resources
    db_manager: Box<dyn DbManager>,
    storage_manager: Box<dyn StorageManager>,
    db_client: Option<Box<dyn DbClient>>,
    http_client: Option<reqwest::blocking::Client>,
    storage_client: Option<Box<dyn StorageClient>>,

    // Usage calculation
    database_write_count: u64,
    database_read_count: u64,
}

impl Instance {
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        id: InstanceID,
        envs: HashMap<String, String>,
        store: Store,
        module: Module,
        memory_limit: byte_unit::Byte,
        giga_instructions_limit: Option<u32>,
        include_logs: bool,
        db_manager: Box<dyn DbManager>,
        storage_manager: Box<dyn StorageManager>,
    ) -> Result<Self> {
        trace!("starting instance {}", id);

        let handle = function::start(store, &module, envs, giga_instructions_limit)?;

        Ok(Instance {
            id,
            handle,

            memory_limit,
            include_logs,

            db_manager,
            storage_manager,
            db_client: None,
            storage_client: None,
            http_client: None,

            database_write_count: 0,
            database_read_count: 0,
        })
    }

    #[inline]
    pub async fn run_request(
        self,
        request: ExecuteFunctionRequest<'static>,
    ) -> ResultWithUsage<(ExecuteFunctionResponse, Usage)> {
        tokio::task::spawn_blocking(move || self.inner_run_request(request))
            .await
            .map_err(|_| {
                (
                    Error::Internal(anyhow!("can not run function task to end")),
                    Default::default(),
                )
            })?
    }

    #[inline]
    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    #[inline]
    fn write_message(&mut self, message: IncomingMessage) -> Result<()> {
        message.write(&mut self.handle.io.stdin).map_err(|e| {
            error!("failed to write data to function: {e}");
            Error::Internal(anyhow!("failed to write data to function {e}",))
        })?;

        Ok(())
    }

    #[inline]
    fn read_message(&mut self) -> Result<OutgoingMessage<'static>> {
        OutgoingMessage::read(&mut self.handle.io.stdout).map_err(Error::FailedToReadMessage)
    }

    fn wait_to_finish_and_get_usage(self) -> ResultWithUsage<Usage> {
        tokio::runtime::Handle::current()
            .block_on(self.handle.join_handle)
            .map(move |metering_points| {
                let usage = |instructions_count| {
                    create_usage(
                        self.database_read_count,
                        self.database_write_count,
                        instructions_count,
                        self.memory_limit,
                    )
                };
                trace!("instance {} finished", &self.id);

                match metering_points {
                    Ok(m) => Ok(usage(m)),
                    Err((e, m)) => Err((e, usage(m))),
                }
            })
            .map_err(|_| {
                (
                    Error::Internal(anyhow!("failed to run function task to end")),
                    Default::default(),
                )
            })?
    }

    #[inline]
    fn inner_run_request(
        mut self,
        request: ExecuteFunctionRequest<'static>,
    ) -> ResultWithUsage<(ExecuteFunctionResponse, Usage)> {
        if self.is_finished() {
            trace!(
                "Instance {} is already exited before sending request",
                &self.id
            );
        }

        self.write_message(IncomingMessage::ExecuteFunction(request))
            .map_err(|e| (e, Default::default()))?;

        loop {
            // TODO: make this async? Possible, but needs work in Borsh as well
            trace!("Waiting for Instance {} message", &self.id);
            match self.read_message() {
                Err(Error::FailedToReadMessage(e))
                    if e.kind() == std::io::ErrorKind::InvalidInput =>
                {
                    error!("Function did not write a FunctionResult or FatalError to its stdout before stopping");

                    log!(
                        target: FUNCTION_LOG_TARGET,
                        Level::Error,
                        "{}: {}",
                        self.id,
                        "Function did not write a FunctionResult or FatalError to its stdout before stopping"
                    );

                    return match self.wait_to_finish_and_get_usage() {
                        Ok(u) => {
                            trace!("USAGE: {}", u.function_instructions);
                            Err((Error::FunctionDidntTerminateCleanly, u))
                        }
                        Err((e, u)) => {
                            trace!("USAGE: {}", u.function_instructions);
                            Err((e, u))
                        }
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
                            log!(
                                target: FUNCTION_LOG_TARGET,
                                Level::Error,
                                "{}: {}",
                                self.id,
                                e.error
                            );

                            let error = Error::FunctionRuntimeError(
                                FunctionRuntimeError::FatalError(e.error.into_owned()),
                            );

                            return match self.wait_to_finish_and_get_usage() {
                                Ok(u) => Err((error, u)),
                                Err((_, u)) => Err((error, u)),
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

                        OutgoingMessage::HttpRequest(req) => self.execute_http_request(req)?,

                        // Database requests
                        OutgoingMessage::Put(_)
                        | OutgoingMessage::Get(_)
                        | OutgoingMessage::Delete(_)
                        | OutgoingMessage::DeleteByPrefix(_)
                        | OutgoingMessage::Scan(_)
                        | OutgoingMessage::ScanKeys(_)
                        | OutgoingMessage::TableList(_)
                        | OutgoingMessage::BatchPut(_)
                        | OutgoingMessage::BatchGet(_)
                        | OutgoingMessage::BatchDelete(_)
                        | OutgoingMessage::BatchScan(_)
                        | OutgoingMessage::BatchScanKeys(_)
                        | OutgoingMessage::CompareAndSwap(_) => self.handle_db_request(message)?,

                        OutgoingMessage::StoragePut(req) => {
                            self.storage_request(|client, stack_id| async move {
                                client
                                    .put(
                                        stack_id,
                                        &req.storage_name,
                                        &req.key,
                                        req.reader.deref().borrow_mut(),
                                    )
                                    .await
                                    .map(|()| {
                                        IncomingMessage::StorageEmptyResult(StorageEmptyResult)
                                    })
                            })?
                        }
                        OutgoingMessage::StorageGet(req) => {
                            self.storage_request(|client, stack_id| async move {
                                let mut data: Vec<u8> = vec![];
                                client
                                    .get(stack_id, &req.storage_name, &req.key, &mut data)
                                    .await
                                    .map(move |()| {
                                        IncomingMessage::StorageGetResult(StorageGetResult {
                                            data: Cow::Owned(data),
                                        })
                                    })
                            })?
                        }
                        OutgoingMessage::StorageDelete(req) => {
                            self.storage_request(|client, stack_id| async move {
                                client
                                    .delete(stack_id, &req.storage_name, &req.key)
                                    .await
                                    .map(|()| {
                                        IncomingMessage::StorageEmptyResult(StorageEmptyResult)
                                    })
                            })?
                        }
                        OutgoingMessage::StorageList(req) => {
                            self.storage_request(|client, stack_id| async move {
                                client
                                    .list(stack_id, &req.storage_name, &req.prefix)
                                    .await
                                    .map(|res| {
                                        IncomingMessage::ObjectListResult(ObjectListResult {
                                            list: res
                                                .into_iter()
                                                .map(|o| incoming_message::storage::Object {
                                                    key: Cow::Owned(o.key),
                                                    size: o.size,
                                                })
                                                .collect(),
                                        })
                                    })
                            })?
                        }
                    }
                }
            }
        }
    }

    fn handle_db_request(&mut self, request: OutgoingMessage) -> ResultWithUsage<()> {
        use database::*;
        let result = match request {
            OutgoingMessage::Put(req) => {
                self.execute_db_request(|db_client, stack_id| async move {
                    let key = make_mudb_key(stack_id, req.table, req.key)?;
                    db_client
                        .put(key, req.value.into_owned(), req.is_atomic)
                        .await
                        .map(into_empty_incoming_msg)
                })
            }

            OutgoingMessage::Get(req) => {
                self.execute_db_request(|db_client, stack_id| async move {
                    let key = make_mudb_key(stack_id, req.table, req.key)?;
                    db_client
                        .get(key)
                        .await
                        .map(into_single_or_empty_incoming_msg)
                })
            }

            OutgoingMessage::Delete(req) => {
                self.execute_db_request(|db_client, stack_id| async move {
                    let key = make_mudb_key(stack_id, req.table, req.key)?;
                    db_client
                        .delete(key, req.is_atomic)
                        .await
                        .map(into_empty_incoming_msg)
                })
            }

            OutgoingMessage::DeleteByPrefix(req) => {
                self.execute_db_request(|db_client, stack_id| async move {
                    let table_name = req.table.into_owned().try_into()?;
                    let key_prefix = req.key_prefix.into_owned();
                    db_client
                        .delete_by_prefix(stack_id, table_name, key_prefix)
                        .await
                        .map(into_empty_incoming_msg)
                })
            }

            OutgoingMessage::Scan(req) => {
                self.execute_db_request(|db_client, stack_id| async move {
                    let db_key = make_mudb_scan(stack_id, req.table, req.key_prefix)?;
                    db_client
                        .scan(db_key, req.limit)
                        .await
                        .map(into_kv_pairs_incoming_msg)
                })
            }

            OutgoingMessage::ScanKeys(req) => {
                self.execute_db_request(|db_client, stack_id| async move {
                    let mudb_scan = make_mudb_scan(stack_id, req.table, req.key_prefix)?;
                    let mudb_keys_to_inner_keys =
                        |k: Vec<mu_db::Key>| k.into_iter().map(|k| k.inner_key);
                    db_client
                        .scan_keys(mudb_scan, req.limit)
                        .await
                        .map(mudb_keys_to_inner_keys)
                        .map(into_list_incoming_msg)
                })
            }

            OutgoingMessage::BatchPut(req) => {
                self.execute_db_request(|db_client, stack_id| async move {
                    let into_mudb_kv_pair = |x: (_, _, Cow<[u8]>)| {
                        let table = x.0;
                        let key = x.1;
                        let value = x.2;
                        make_mudb_key(stack_id, table, key)
                            .map(|mudb_key| (mudb_key, value.into_owned()))
                    };
                    let mudb_kv_pairs = req
                        .table_key_value_triples
                        .into_iter()
                        .map(into_mudb_kv_pair)
                        .collect::<mu_db::error::Result<_>>()?;
                    db_client
                        .batch_put(mudb_kv_pairs, req.is_atomic)
                        .await
                        .map(into_empty_incoming_msg)
                })
            }

            OutgoingMessage::BatchGet(req) => {
                self.execute_db_request(|db_client, stack_id| async move {
                    let keys = make_mudb_keys(stack_id, req.table_key_tuples)?;
                    db_client
                        .batch_get(keys)
                        .await
                        .map(into_tkv_triples_incoming_msg)
                })
            }

            OutgoingMessage::BatchDelete(req) => {
                self.execute_db_request(|db_client, stack_id| async move {
                    let keys = make_mudb_keys(stack_id, req.table_key_tuples)?;
                    db_client
                        .batch_delete(keys)
                        .await
                        .map(into_empty_incoming_msg)
                })
            }

            OutgoingMessage::BatchScan(req) => {
                self.execute_db_request(|db_client, stack_id| async move {
                    let scans = make_mudb_scans(stack_id, req.table_key_prefix_tuples)?;
                    db_client
                        .batch_scan(scans, req.each_limit)
                        .await
                        .map(into_tkv_triples_incoming_msg)
                })
            }

            OutgoingMessage::BatchScanKeys(req) => {
                self.execute_db_request(|db_client, stack_id| async move {
                    let scans = make_mudb_scans(stack_id, req.table_key_prefix_tuples)?;
                    db_client
                        .batch_scan_keys(scans, req.each_limit)
                        .await
                        .map(into_tk_pairs_incoming_msg)
                })
            }

            OutgoingMessage::TableList(req) => {
                self.execute_db_request(|db_client, stack_id| async move {
                    let table_name_prefix = Some(req.table_prefix.into_owned().try_into()?);
                    db_client
                        .table_list(stack_id, table_name_prefix)
                        .await
                        .map(into_list_incoming_msg)
                })
            }

            OutgoingMessage::CompareAndSwap(req) => {
                self.execute_db_request(|db_client, stack_id| async move {
                    let key = make_mudb_key(stack_id, req.table, req.key)?;
                    let prev_value = req.previous_value.map(|x| x.into_owned());
                    db_client
                        .compare_and_swap(key, prev_value, req.new_value.into_owned())
                        .await
                        .map(into_cas_incoming_msg)
                })
            }

            // TODO: separate messages into enums containing messages for one system to avoid this
            _ => Err(Error::Internal(anyhow!(
                "invalid request type, only database requests are handled here."
            ))),
        };

        result.map_err(|e| (e, Default::default()))
    }

    fn execute_db_request<'a, A, B>(&mut self, f: A) -> Result<()>
    where
        A: FnOnce(Box<dyn DbClient>, StackID) -> B,
        B: Future<Output = mu_db::error::Result<IncomingMessage<'a>>>,
    {
        tokio::runtime::Handle::current().block_on(async move {
            let stack_id = self.id.function_id.stack_id;

            let client = match self.db_client {
                Some(ref client) => client.clone(),
                None => match self.db_manager.make_client().await {
                    Ok(client) => {
                        self.db_client = Some(client.clone());
                        client
                    }
                    Err(e) => {
                        self.write_message(IncomingMessage::DbError(DbError {
                            error: Cow::Owned(e.to_string()),
                        }))?;
                        return Ok(()); //TODO: is it okay that runtime will not do anything about
                                       //this and only returns the error to users function?
                                       // @Arshia001
                    }
                },
            };

            let msg = f(client, stack_id).await.unwrap_or_else(|e| {
                IncomingMessage::DbError(DbError {
                    error: Cow::from(format!("{e:?}")),
                })
            });
            self.write_message(msg)
        })
    }

    fn execute_http_request(
        &mut self,
        req: musdk_common::http_client::Request,
    ) -> ResultWithUsage<()> {
        use http_client::*;

        if self.http_client.is_none() {
            self.http_client = Some(reqwest::blocking::Client::new());
        }

        let mut request = self
            .http_client
            .as_ref()
            .unwrap()
            .request(http_method_to_reqwest_method(req.method), req.url)
            .version(version_to_reqwest_version(req.version));

        for header in req.headers {
            request = request.header(header.name.as_ref(), header.value.as_ref());
            request = request.body(req.body.to_vec());
        }

        let response = reqwest_response_to_http_response(request.send());
        let message = IncomingMessage::HttpResponse(response);
        self.write_message(message)
            .map_err(|e| (e, Usage::default()))?;

        Ok(())
    }
    fn storage_request<'a, A, B>(&mut self, f: A) -> Result<(), (Error, Usage)>
    where
        A: FnOnce(Box<dyn StorageClient>, StackID) -> B,
        B: Future<Output = anyhow::Result<IncomingMessage<'a>>>,
    {
        tokio::runtime::Handle::current().block_on(async {
            let stack_id = self.id.function_id.stack_id;
            let storage_client_res = match &self.storage_client {
                Some(client) => Ok(client.clone()),
                None => {
                    let client = self.storage_manager.make_client();
                    self.storage_client = client.as_ref().ok().map(ToOwned::to_owned);
                    client
                }
            };

            match storage_client_res {
                Ok(client) => {
                    let msg = f(client, stack_id).await.unwrap_or_else(|e| {
                        IncomingMessage::StorageError(StorageError {
                            error: Cow::from(format!("{e:?}")),
                        })
                    });
                    self.write_message(msg)
                }
                Err(e) => self.write_message(IncomingMessage::StorageError(StorageError {
                    error: Cow::from(format!("{e:?}")),
                })),
            }
            .map_err(|e| (e, Usage::default()))
        })
    }
}
