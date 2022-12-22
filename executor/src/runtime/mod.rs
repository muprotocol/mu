//TODO
#![allow(dead_code)]
//TODO: Add logging

pub mod error;
pub mod function;
pub mod instance;
pub mod memory;
pub mod packet;
pub mod providers;
pub mod types;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use dyn_clonable::clonable;
use log::*;
use mailbox_processor::{callback::CallbackMailboxProcessor, ReplyChannel};
use mu_stack::StackID;
use std::{collections::HashMap, path::Path, sync::Arc};
use wasmer::{Module, Store};
use wasmer_cache::{Cache, FileSystemCache};

use self::{
    error::Error,
    instance::{create_store, Instance, Loaded},
    types::{
        FunctionDefinition, FunctionID, FunctionProvider, InvokeFunctionRequest, RuntimeConfig,
    },
};
use crate::{
    gateway, mudb::service::DatabaseManager, runtime::error::FunctionLoadingError,
    stack::usage_aggregator::UsageAggregator,
};

#[async_trait]
#[clonable]
pub trait Runtime: Clone + Send + Sync {
    async fn invoke_function<'a>(
        &self,
        function_id: FunctionID,
        request: gateway::Request<'a>,
    ) -> Result<gateway::Response, Error>;

    async fn stop(&self) -> Result<(), Error>;

    async fn add_functions(&self, functions: Vec<FunctionDefinition>) -> Result<(), Error>;
    async fn remove_functions(&self, stack_id: StackID, names: Vec<String>) -> Result<(), Error>;
    async fn get_function_names(&self, stack_id: StackID) -> Result<Vec<String>, Error>;
}

#[derive(Debug)]
pub enum MailboxMessage<'a> {
    InvokeFunction(InvokeFunctionRequest),
    Shutdown,

    AddFunctions(Vec<FunctionDefinition>),
    RemoveFunctions(StackID, Vec<String>),
    GetFunctionNames(StackID, ReplyChannel<Vec<String>>),
}

#[derive(Clone)]
struct RuntimeImpl {
    mailbox: CallbackMailboxProcessor<MailboxMessage>>,
}

struct CacheHashAndMemoryLimit {
    hash: wasmer_cache::Hash,
    memory_limit: byte_unit::Byte,
}

struct RuntimeState {
    function_provider: Box<dyn FunctionProvider>,
    hashkey_dict: HashMap<FunctionID, CacheHashAndMemoryLimit>,
    cache: FileSystemCache,
    database_service: Arc<DatabaseManager>,
    usage_aggregator: Box<dyn UsageAggregator>,
}

impl RuntimeState {
    pub async fn new(
        function_provider: Box<dyn FunctionProvider>,
        cache_path: &Path,
        database_service: DatabaseManager,
        usage_aggregator: Box<dyn UsageAggregator>,
    ) -> Result<Self> {
        let hashkey_dict = HashMap::new();
        let mut cache = FileSystemCache::new(cache_path).context("failed to create cache")?;
        cache.set_cache_extension(Some("wasmu"));

        Ok(Self {
            hashkey_dict,
            function_provider,
            cache,
            database_service: Arc::new(database_service),
            usage_aggregator,
        })
    }

    fn load_module(&mut self, function_id: &FunctionID) -> Result<(Store, Module)> {
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

                    let definition = self.function_provider.get(function_id).ok_or_else(|| {
                        Error::FunctionLoadingError(FunctionLoadingError::FunctionNotFound(
                            function_id.clone(),
                        ))
                    })?;

                    let module = Module::new(&store, definition.source.clone())?;

                    self.cache.store(*hash, &module)?;

                    Ok((store, module))
                }
            }
        } else {
            let function_definition = match self.function_provider.get(function_id) {
                Some(d) => d,
                None => {
                    return Err(Error::FunctionLoadingError(
                        FunctionLoadingError::FunctionNotFound(function_id.clone()),
                    )
                    .into());
                }
            };

            let mut hash_array = Vec::with_capacity(function_id.function_name.len() + 16); // Uuid is 16 bytes
            hash_array.extend_from_slice(function_id.stack_id.get_bytes()); //This is bad, should
                                                                            //use a method on
                                                                            //StackID
            hash_array.extend_from_slice(function_id.function_name.as_bytes());
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
                    Error::FunctionLoadingError(FunctionLoadingError::InvalidFunctionModule(
                        function_id.clone(),
                    ))
                    .into(),
                )
            }
        }
    }

    async fn instantiate_function(&mut self, function_id: FunctionID) -> Result<Instance<Loaded>> {
        trace!("instantiate function {}", function_id);
        let definition = self
            .function_provider
            .get(&function_id)
            .ok_or_else(|| {
                Error::FunctionLoadingError(FunctionLoadingError::FunctionNotFound(
                    function_id.clone(),
                ))
            })?
            .to_owned();

        trace!("loading function {}", function_id);
        let (store, module) = self.load_module(&function_id)?;
        Ok(Instance::new(
            function_id,
            definition.envs,
            store,
            module,
            self.database_service.clone(),
        ))
    }
}

#[async_trait]
impl Runtime for RuntimeImpl {
    async fn invoke_function<'a>(
        &self,
        function_id: FunctionID,
        request: gateway::Request<'a>,
    ) -> Result<gateway::Response, Error> {
        let request = packet::gateway::Request::new(request);

        self.mailbox
            .post_and_reply(|r| {
                MailboxMessage::InvokeFunction(InvokeFunctionRequest {
                    function_id,
                    request,
                    reply: r,
                })
            })
            .await
            .map_err(|e| Error::Internal(e.into()))?
            .map(|r| r.0)
    }

    async fn stop(&self) -> Result<(), Error> {
        self.mailbox
            .post(MailboxMessage::Shutdown)
            .await
            .map_err(|e| Error::Internal(e.into()))?;
        self.mailbox.clone().stop().await;
        Ok(())
    }

    async fn add_functions(&self, functions: Vec<FunctionDefinition>) -> Result<(), Error> {
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
    function_provider: Box<dyn FunctionProvider>,
    config: RuntimeConfig,
    db_service: DatabaseManager,
    usage_aggregator: Box<dyn UsageAggregator>,
) -> Result<Box<dyn Runtime>> {
    let state = RuntimeState::new(
        function_provider,
        &config.cache_path,
        db_service,
        usage_aggregator,
    )
    .await?;
    let mailbox = CallbackMailboxProcessor::start(mailbox_step, state, 10000);
    Ok(Box::new(RuntimeImpl { mailbox }))
}

async fn mailbox_step(
    _mb: CallbackMailboxProcessor<MailboxMessage>,
    msg: MailboxMessage,
    mut state: RuntimeState,
) -> RuntimeState {
    match msg {
        MailboxMessage::InvokeFunction(req) => {
            if let Ok(instance) = state.instantiate_function(req.function_id.clone()).await {
                let memory_limit = state
                    .hashkey_dict
                    .get(&req.function_id)
                    .unwrap()
                    .memory_limit;

                let usage_aggregator = state.usage_aggregator.clone();

                tokio::spawn(async move {
                    match instance.start() {
                        Err(e) => req.reply.reply(Err(e)),
                        Ok(i) => {
                            let result = i
                                .run_request(memory_limit, req.request)
                                .await
                                .map(|(resp, usages)| {
                                    usage_aggregator
                                        .register_usage(req.function_id.stack_id, usages);
                                    resp
                                })
                                .map_err(|(error, usages)| {
                                    usage_aggregator
                                        .register_usage(req.function_id.stack_id, usages);
                                    error
                                });

                            req.reply.reply(result);
                        }
                    }
                });
            } else {
                req.reply.reply(
                    Err(Error::Internal(anyhow!("Can not instantiate function")))
                        .map_err(Into::into),
                )
            }
        }

        MailboxMessage::Shutdown => {
            //TODO: find a way to kill running functions
        }

        MailboxMessage::AddFunctions(functions) => {
            for f in functions {
                state.function_provider.add_function(f);
            }
        }

        MailboxMessage::RemoveFunctions(stack_id, functions_names) => {
            for function_name in functions_names {
                let function_id = FunctionID {
                    stack_id,
                    function_name,
                };

                state.function_provider.remove_function(&function_id);
                state.hashkey_dict.remove(&function_id);
            }
        }

        MailboxMessage::GetFunctionNames(stack_id, r) => {
            r.reply(state.function_provider.get_function_names(&stack_id));
        }
    }
    state
}
