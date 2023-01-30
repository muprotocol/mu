pub mod error;
pub mod function;
pub mod instance;
pub mod memory;
pub mod providers;
mod types;

use std::{
    borrow::Cow,
    collections::HashMap,
    ops::{Add, AddAssign},
};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use dyn_clonable::clonable;
use log::*;
use tokio::sync::mpsc;
use wasmer::{Module, Store};
use wasmer_cache::{Cache, FileSystemCache};

use mailbox_processor::{callback::CallbackMailboxProcessor, NotificationChannel, ReplyChannel};
use mu_common::id::IdExt;
use mu_stack::{AssemblyID, FunctionID, StackID};
use musdk_common::{Header, Request, Response};

use instance::create_store;

pub use error::{Error, FunctionLoadingError, FunctionRuntimeError};
pub use instance::{Instance, Loaded, Running};
pub use types::{AssemblyDefinition, AssemblyProvider, InvokeFunctionRequest, RuntimeConfig};

#[async_trait]
#[clonable]
pub trait Runtime: Clone + Send + Sync {
    async fn invoke_function<'a>(
        &self,
        function_id: FunctionID,
        request: Request<'a>,
    ) -> Result<Response<'static>, Error>;

    async fn stop(&self) -> Result<(), Error>;

    async fn add_functions(&self, functions: Vec<AssemblyDefinition>) -> Result<(), Error>;
    async fn remove_functions(&self, stack_id: StackID, names: Vec<String>) -> Result<(), Error>;
    async fn get_function_names(&self, stack_id: StackID) -> Result<Vec<String>, Error>;
}

#[derive(Clone)]
pub enum Notification {
    ReportUsage(StackID, Usage),
}

#[derive(Default, Clone)]
pub struct Usage {
    pub db_weak_reads: u64,
    pub db_strong_reads: u64,
    pub db_weak_writes: u64,
    pub db_strong_writes: u64,
    pub function_instructions: u64,
    pub memory_megabytes: u64,
}

impl Add for Usage {
    type Output = Self;

    fn add(mut self, rhs: Self) -> Self::Output {
        self.db_weak_reads += rhs.db_weak_reads;
        self.db_weak_writes += rhs.db_weak_writes;
        self.db_strong_reads += rhs.db_strong_reads;
        self.db_strong_writes += rhs.db_strong_writes;
        self.function_instructions += rhs.function_instructions;
        self.memory_megabytes += rhs.memory_megabytes;
        self
    }
}

impl AddAssign for Usage {
    fn add_assign(&mut self, rhs: Self) {
        self.db_weak_reads += rhs.db_weak_reads;
        self.db_weak_writes += rhs.db_weak_writes;
        self.db_strong_reads += rhs.db_strong_reads;
        self.db_strong_writes += rhs.db_strong_writes;
        self.function_instructions += rhs.function_instructions;
        self.memory_megabytes += rhs.memory_megabytes;
    }
}

#[derive(Debug)]
enum MailboxMessage {
    InvokeFunction(InvokeFunctionRequest),
    Shutdown,

    AddFunctions(Vec<AssemblyDefinition>),
    RemoveFunctions(StackID, Vec<String>),
    GetFunctionNames(StackID, ReplyChannel<Vec<String>>),
}

#[derive(Clone)]
struct RuntimeImpl {
    mailbox: CallbackMailboxProcessor<MailboxMessage>,
}

struct CacheHashAndMemoryLimit {
    hash: wasmer_cache::Hash,
    memory_limit: byte_unit::Byte,
}

struct RuntimeState {
    config: RuntimeConfig,
    assembly_provider: Box<dyn AssemblyProvider>,
    hashkey_dict: HashMap<AssemblyID, CacheHashAndMemoryLimit>,
    cache: FileSystemCache,
    next_instance_id: u64,
    notification_channel: NotificationChannel<Notification>,
}

impl RuntimeState {
    pub async fn new(
        assembly_provider: Box<dyn AssemblyProvider>,
        config: RuntimeConfig,
    ) -> Result<(Self, mpsc::UnboundedReceiver<Notification>)> {
        let (tx, rx) = NotificationChannel::new();

        let hashkey_dict = HashMap::new();
        let mut cache =
            FileSystemCache::new(&config.cache_path).context("failed to create cache")?;
        cache.set_cache_extension(Some("wasmu"));

        Ok((
            Self {
                config,
                hashkey_dict,
                assembly_provider,
                cache,
                next_instance_id: 0,
                notification_channel: tx,
            },
            rx,
        ))
    }

    fn load_module(&mut self, function_id: &AssemblyID) -> Result<(Store, Module)> {
        if self.hashkey_dict.contains_key(function_id) {
            let CacheHashAndMemoryLimit { hash, memory_limit } = self
                .hashkey_dict
                .get(function_id)
                .ok_or_else(|| Error::Internal(anyhow!("cache key can not be found")))?
                .to_owned();

            let store = create_store(*memory_limit)?;

            match unsafe { self.cache.load(&store, *hash) } {
                Ok(module) => Ok((store, module)),
                Err(e) => {
                    warn!("cached module is corrupted: {}", e);

                    let definition = self.assembly_provider.get(function_id).ok_or_else(|| {
                        Error::FunctionLoadingError(FunctionLoadingError::AssemblyNotFound(
                            function_id.clone(),
                        ))
                    })?;

                    let module = Module::new(&store, definition.source.clone())?;

                    self.cache.store(*hash, &module)?;

                    Ok((store, module))
                }
            }
        } else {
            let function_definition = match self.assembly_provider.get(function_id) {
                Some(d) => d,
                None => {
                    return Err(Error::FunctionLoadingError(
                        FunctionLoadingError::AssemblyNotFound(function_id.clone()),
                    )
                    .into());
                }
            };

            let mut hash_array = Vec::with_capacity(function_id.assembly_name.len() + 16); // Uuid is 16 bytes
            hash_array.extend_from_slice(function_id.stack_id.get_bytes()); //This is bad, should
                                                                            //use a method on
                                                                            //StackID
            hash_array.extend_from_slice(function_id.assembly_name.as_bytes());
            let hash = wasmer_cache::Hash::generate(&hash_array);

            self.hashkey_dict.insert(
                function_id.clone(),
                CacheHashAndMemoryLimit {
                    hash,
                    memory_limit: function_definition.memory_limit,
                },
            );

            let store = create_store(function_definition.memory_limit)?;

            if let Ok(module) = Module::from_binary(&store, &function_definition.source) {
                if let Err(e) = self.cache.store(hash, &module) {
                    error!("failed to cache module: {e}, function id: {}", function_id);
                }
                Ok((store, module))
            } else {
                error!("can not build wasm module for function: {}", function_id);
                Err(
                    Error::FunctionLoadingError(FunctionLoadingError::InvalidAssembly(
                        function_id.clone(),
                    ))
                    .into(),
                )
            }
        }
    }

    async fn instantiate_function(&mut self, assembly_id: AssemblyID) -> Result<Instance<Loaded>> {
        trace!("instantiate function {}", assembly_id);
        let definition = self
            .assembly_provider
            .get(&assembly_id)
            .ok_or_else(|| {
                Error::FunctionLoadingError(FunctionLoadingError::AssemblyNotFound(
                    assembly_id.clone(),
                ))
            })?
            .to_owned();

        trace!("loading function {}", assembly_id);
        let (store, module) = self.load_module(&assembly_id)?;
        Ok(Instance::new(
            assembly_id,
            self.next_instance_id.get_and_increment(),
            definition.envs,
            store,
            module,
            definition.memory_limit,
            self.config.include_function_logs,
        ))
    }
}

#[async_trait]
impl Runtime for RuntimeImpl {
    async fn invoke_function<'a>(
        &self,
        function_id: FunctionID,
        request: Request<'a>,
    ) -> Result<Response<'static>, Error> {
        // TODO: This is a rather ridiculous thing to do, but necessary
        // since we're sending the request to another thread. There has
        // to be a better way.
        let request = musdk_common::incoming_message::ExecuteFunction {
            function: Cow::Owned(function_id.function_name),
            request: Request {
                method: request.method,
                path_params: request
                    .path_params
                    .into_iter()
                    .map(|(k, v)| (Cow::Owned(k.into_owned()), Cow::Owned(v.into_owned())))
                    .collect(),
                query_params: request
                    .query_params
                    .into_iter()
                    .map(|(k, v)| (Cow::Owned(k.into_owned()), Cow::Owned(v.into_owned())))
                    .collect(),
                headers: request
                    .headers
                    .into_iter()
                    .map(|h| Header {
                        name: Cow::Owned(h.name.into_owned()),
                        value: Cow::Owned(h.value.into_owned()),
                    })
                    .collect(),
                body: Cow::Owned(request.body.into_owned()),
            },
        };

        let response = self
            .mailbox
            .post_and_reply(|r| {
                MailboxMessage::InvokeFunction(InvokeFunctionRequest {
                    assembly_id: function_id.assembly_id,
                    request,
                    reply: r,
                })
            })
            .await
            .map_err(|e| Error::Internal(e.into()))??;
        Ok(response.response)
    }

    async fn stop(&self) -> Result<(), Error> {
        self.mailbox
            .post(MailboxMessage::Shutdown)
            .await
            .map_err(|e| Error::Internal(e.into()))?;
        self.mailbox.clone().stop().await;
        Ok(())
    }

    async fn add_functions(&self, functions: Vec<AssemblyDefinition>) -> Result<(), Error> {
        self.mailbox
            .post(MailboxMessage::AddFunctions(functions))
            .await
            .map_err(|e| Error::Internal(e.into()))
    }

    async fn remove_functions(&self, stack_id: StackID, names: Vec<String>) -> Result<(), Error> {
        self.mailbox
            .post(MailboxMessage::RemoveFunctions(stack_id, names))
            .await
            .map_err(|e| Error::Internal(e.into()))
    }

    async fn get_function_names(&self, stack_id: StackID) -> Result<Vec<String>, Error> {
        self.mailbox
            .post_and_reply(|r| MailboxMessage::GetFunctionNames(stack_id, r))
            .await
            .map_err(|e| Error::Internal(e.into()))
    }
}

pub async fn start(
    assembly_provider: Box<dyn AssemblyProvider>,
    config: RuntimeConfig,
) -> Result<(Box<dyn Runtime>, mpsc::UnboundedReceiver<Notification>)> {
    let (state, notification_receiver) = RuntimeState::new(assembly_provider, config).await?;
    let mailbox = CallbackMailboxProcessor::start(mailbox_step, state, 10000);
    Ok((Box::new(RuntimeImpl { mailbox }), notification_receiver))
}

async fn mailbox_step(
    _mb: CallbackMailboxProcessor<MailboxMessage>,
    msg: MailboxMessage,
    mut state: RuntimeState,
) -> RuntimeState {
    let notification_channel = state.notification_channel.clone();

    match msg {
        MailboxMessage::InvokeFunction(req) => {
            match state.instantiate_function(req.assembly_id.clone()).await {
                Ok(instance) => {
                    tokio::spawn(async move {
                        match instance.start() {
                            Err(e) => req.reply.reply(Err(e)),
                            Ok(i) => {
                                let result = i
                                    .run_request(req.request)
                                    .await
                                    .map(|(resp, usages)| {
                                        notification_channel.send(Notification::ReportUsage(
                                            req.assembly_id.stack_id,
                                            usages,
                                        ));
                                        resp
                                    })
                                    .map_err(|(error, usages)| {
                                        notification_channel.send(Notification::ReportUsage(
                                            req.assembly_id.stack_id,
                                            usages,
                                        ));
                                        error
                                    });

                                req.reply.reply(result);
                            }
                        }
                    });
                }
                Err(f) => req.reply.reply(Err(Error::Internal(f))),
            }
        }

        MailboxMessage::Shutdown => {
            //TODO: find a way to kill running functions
        }

        MailboxMessage::AddFunctions(functions) => {
            for f in functions {
                state.assembly_provider.add_function(f);
            }
        }

        MailboxMessage::RemoveFunctions(stack_id, functions_names) => {
            for function_name in functions_names {
                let function_id = AssemblyID {
                    stack_id,
                    assembly_name: function_name,
                };

                state.assembly_provider.remove_function(&function_id);
                state.hashkey_dict.remove(&function_id);
            }
        }

        MailboxMessage::GetFunctionNames(stack_id, r) => {
            r.reply(state.assembly_provider.get_function_names(&stack_id));
        }
    }
    state
}
