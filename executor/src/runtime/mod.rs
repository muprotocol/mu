//TODO
#![allow(dead_code)]
//TODO: Add logging

pub mod error;
pub mod function;
pub mod message;
pub mod providers;
pub mod types;

const MESSAGE_READ_BUF_CAP: usize = 8 * 1024;
const FUNCTION_TERM_TIMEOUT: Duration = Duration::from_secs(2);

use self::{
    error::Error,
    message::{
        database::{database_id, DbRequest, DbResponse, DbResponseDetails},
        gateway::{GatewayRequest, GatewayResponse},
        log::Log,
        FromMessage, Message,
    },
    types::{
        FunctionDefinition, FunctionHandle, FunctionID, FunctionProvider, FunctionUsage,
        InstanceID, InvokeFunctionRequest,
    },
};
use crate::{
    gateway,
    mu_stack::StackID,
    mudb::{self, service as DbService},
    runtime::message::{database::DbRequestDetails, ToMessage},
};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use bytes::BufMut;
use dyn_clonable::clonable;
use futures::Future;
use mailbox_processor::{callback::CallbackMailboxProcessor, ReplyChannel};
use std::{
    io::{BufRead, Write},
    time::Duration,
};
use wasmer_middlewares::metering::MeteringPoints;

#[async_trait]
#[clonable]
pub trait Runtime: Clone + Send + Sync {
    async fn invoke_function<'a>(
        &self,
        function_id: FunctionID,
        message: gateway::Request<'a>,
    ) -> Result<(gateway::Response, FunctionUsage)>;

    async fn shutdown(&self) -> Result<()>;

    async fn add_functions(&mut self, functions: Vec<FunctionDefinition>) -> Result<()>;
    async fn remove_functions(&mut self, stack_id: StackID, names: Vec<String>) -> Result<()>;
    async fn get_function_names(&self, stack_id: StackID) -> Result<Vec<String>>;
}

#[derive(Debug)]
pub enum MailboxMessage {
    InvokeFunction(InvokeFunctionRequest),
    Shutdown,

    AddFunctions(Vec<FunctionDefinition>),
    RemoveFunctions(StackID, Vec<String>),
    GetFunctionNames(StackID, ReplyChannel<Vec<String>>),
}

//TODO:
// * use metrics and MemoryUsage so we can report usage of memory and CPU time.
// * remove less frequently used source's from runtime
#[derive(Clone)]
struct RuntimeImpl {
    mailbox: CallbackMailboxProcessor<MailboxMessage>,
}

struct RuntimeState {
    function_provider: Box<dyn FunctionProvider>,
}

impl RuntimeState {
    async fn instantiate_function(&mut self, function_id: FunctionID) -> Result<Instance> {
        let definition = self
            .function_provider
            .get(&function_id)
            .ok_or(Error::FunctionNotFound(function_id))?;
        let instance = Instance::new(definition)?;
        Ok(instance)
    }
}

#[async_trait]
impl Runtime for RuntimeImpl {
    async fn invoke_function<'a>(
        &self,
        function_id: FunctionID,
        message: gateway::Request<'a>,
    ) -> Result<(gateway::Response, FunctionUsage)> {
        let message = GatewayRequest::new(message)
            .to_message()
            .context("Failed to serialize request message")?;
        let result = self
            .mailbox
            .post_and_reply(|r| {
                MailboxMessage::InvokeFunction(InvokeFunctionRequest {
                    function_id,
                    message,
                    reply: r,
                })
            })
            .await;

        match result {
            Ok(r) => r.map(|r| (r.0.response, r.1)),
            Err(e) => Err(e).map_err(Into::into),
        }
    }

    async fn shutdown(&self) -> Result<()> {
        self.mailbox.post(MailboxMessage::Shutdown).await?;
        self.mailbox.clone().stop().await;
        Ok(())
    }

    async fn add_functions(&mut self, functions: Vec<FunctionDefinition>) -> Result<()> {
        self.mailbox
            .post(MailboxMessage::AddFunctions(functions))
            .await
            .map_err(Into::into)
    }

    async fn remove_functions(&mut self, stack_id: StackID, names: Vec<String>) -> Result<()> {
        self.mailbox
            .post(MailboxMessage::RemoveFunctions(stack_id, names))
            .await
            .map_err(Into::into)
    }

    async fn get_function_names(&self, stack_id: StackID) -> Result<Vec<String>> {
        self.mailbox
            .post_and_reply(|r| MailboxMessage::GetFunctionNames(stack_id, r))
            .await
            .map_err(Into::into)
    }
}

#[derive(Debug)]
pub struct Instance {
    id: InstanceID,
    handle: FunctionHandle,
}

impl Instance {
    pub fn new(definition: &FunctionDefinition) -> Result<Self> {
        let handle = function::start(definition)?;

        Ok(Self {
            handle,
            id: InstanceID::generate_random(definition.id.clone()),
        })
    }

    pub fn is_finished(&self) -> bool {
        self.handle.join_handle.is_finished()
    }

    fn write_to_stdin(&mut self, input: Message) -> Result<()> {
        let mut bytes = input.as_bytes()?;
        bytes.put_u8(b'\n');
        self.handle.io.stdin.write_all(&bytes)?;
        self.handle.io.stdin.flush()?;
        Ok(())
    }

    fn read_from_stdout(&mut self) -> Result<Message> {
        let mut buf = String::with_capacity(MESSAGE_READ_BUF_CAP);
        loop {
            let bytes_read = self.handle.io.stdout.read_line(&mut buf)?;
            if bytes_read == 0 {
                continue;
            };

            return serde_json::from_slice(buf.as_bytes()).map_err(Into::into);
        }
    }

    //if self.cancellation.token.is_cancelled() {
    //    println!("Got shutdown");
    //    let msg = Signal::term().to_message().unwrap();
    //    self.write_to_stdin(msg)?;
    //    println!("shutdown signal sent");
    //    std::thread::sleep(FUNCTION_TERM_TIMEOUT);
    //    println!("shutdown timeout");
    //    self.handle.join_handle.abort();
    //    println!("function aborted");
    //}

    pub fn request(
        mut self,
        request: Message,
    ) -> Result<impl Future<Output = Result<(GatewayResponse, FunctionUsage)>>> {
        //TODO: check function state
        self.write_to_stdin(request)?;
        loop {
            if self.is_finished() {
                return Err(Error::FunctionEarlyExit(self.id)).map_err(Into::into);
            }

            match self.read_from_stdout() {
                Ok(message) => match message.r#type.as_str() {
                    GatewayResponse::TYPE => {
                        let resp = GatewayResponse::from_message(message)?;
                        return Ok(async move {
                            match self.handle.join_handle.await {
                                Ok(MeteringPoints::Exhausted) => Ok((resp, u64::MAX)),
                                Ok(MeteringPoints::Remaining(p)) => Ok((resp, u64::MAX - p)),
                                Err(_) => {
                                    Err(Error::FunctionAborted(self.id.clone())).map_err(Into::into)
                                }
                            }
                        });
                    }

                    DbRequest::TYPE => {
                        let db_req = DbRequest::from_message(message)?;
                        let db_resp = match db_req.request {
                            DbRequestDetails::CreateTable(req) => {
                                let res = tokio::runtime::Handle::current()
                                    .block_on(DbService::create_table(
                                        database_id(&self.id.function_id, req.db_name),
                                        req.table_name,
                                    ))
                                    .map_err(|e| e.to_string());

                                DbResponse {
                                    id: db_req.id,
                                    response: DbResponseDetails::CreateTable(res),
                                }
                            }

                            DbRequestDetails::DropTable(req) => {
                                let res = tokio::runtime::Handle::current()
                                    .block_on(DbService::delete_table(
                                        database_id(&self.id.function_id, req.db_name),
                                        req.table_name,
                                    ))
                                    .map_err(|e| e.to_string());

                                DbResponse {
                                    id: db_req.id,
                                    response: DbResponseDetails::DropTable(res),
                                }
                            }

                            DbRequestDetails::Find(req) => {
                                let res = tokio::runtime::Handle::current()
                                    .block_on(DbService::find_item(
                                        database_id(&self.id.function_id, req.db_name),
                                        req.table_name,
                                        req.key_filter,
                                        req.value_filter,
                                    ))
                                    .map_err(|e| e.to_string());

                                DbResponse {
                                    id: db_req.id,
                                    response: DbResponseDetails::Find(res),
                                }
                            }
                            DbRequestDetails::Insert(req) => {
                                let res = tokio::runtime::Handle::current()
                                    .block_on({
                                        DbService::insert_one_item(
                                            database_id(&self.id.function_id, req.db_name),
                                            req.table_name,
                                            req.key,
                                            req.value,
                                        )
                                    })
                                    .map_err(|e| e.to_string());

                                DbResponse {
                                    id: db_req.id,
                                    response: DbResponseDetails::Insert(res),
                                }
                            }
                            DbRequestDetails::Update(req) => {
                                let res = tokio::runtime::Handle::current()
                                    .block_on(DbService::update_item(
                                        database_id(&self.id.function_id, req.db_name),
                                        req.table_name,
                                        req.key_filter,
                                        req.value_filter,
                                        mudb::query::Update(req.update),
                                    ))
                                    .map_err(|e| e.to_string());

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

pub fn start(function_provider: Box<dyn FunctionProvider>) -> Box<dyn Runtime> {
    let state = RuntimeState { function_provider };

    let mailbox = CallbackMailboxProcessor::start(mailbox_step, state, 10000);

    Box::new(RuntimeImpl { mailbox })
}

async fn mailbox_step(
    _mb: CallbackMailboxProcessor<MailboxMessage>,
    msg: MailboxMessage,
    mut runtime: RuntimeState,
) -> RuntimeState {
    //TODO: pass metering info to blockchain_manager service
    match msg {
        MailboxMessage::InvokeFunction(req) => {
            if let Ok(instance) = runtime.instantiate_function(req.function_id).await {
                tokio::spawn(async {
                    let resp = tokio::task::spawn_blocking(move || instance.request(req.message))
                        .await
                        .unwrap(); // TODO: Handle spawn_blocking errors

                    match resp {
                        Ok(a) => req.reply.reply(a.await),
                        Err(a) => req.reply.reply(Err(a)),
                    };
                });
            }
        }

        MailboxMessage::Shutdown => {
            //TODO: find a way to kill running functions
        }

        MailboxMessage::AddFunctions(functions) => {
            for f in functions {
                runtime.function_provider.add_function(f)
            }
        }

        MailboxMessage::RemoveFunctions(stack_id, functions_names) => {
            for function_name in functions_names {
                runtime.function_provider.remove_function(&FunctionID {
                    stack_id,
                    function_name,
                })
            }
        }

        MailboxMessage::GetFunctionNames(stack_id, r) => {
            r.reply(runtime.function_provider.get_function_names(&stack_id));
        }
    }
    runtime
}
