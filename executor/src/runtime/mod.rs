//TODO
#![allow(dead_code)]
//TODO: Add logging

pub mod error;
pub mod function;
pub mod instance;
pub mod memory;
pub mod message;
pub mod providers;
pub mod types;

use anyhow::{Context, Result};
use async_trait::async_trait;
use dyn_clonable::clonable;
use log::*;
use mailbox_processor::{callback::CallbackMailboxProcessor, ReplyChannel};
use mu_stack::{KiloByte, StackID};
use std::{collections::HashMap, path::Path, sync::Arc};
use wasmer::{Module, Store};
use wasmer_cache::{Cache, FileSystemCache};

use self::{
    error::Error,
    instance::{create_store, Instance, Loaded},
    message::gateway::GatewayRequest,
    types::{
        FunctionDefinition, FunctionID, FunctionProvider, FunctionUsage, InvokeFunctionRequest,
        RuntimeConfig,
    },
};
use crate::{gateway, mudb::service::DatabaseManager, runtime::message::ToMessage};

#[async_trait]
#[clonable]
pub trait Runtime: Clone + Send + Sync {
    async fn invoke_function<'a>(
        &self,
        function_id: FunctionID,
        message: gateway::Request<'a>,
    ) -> Result<(gateway::Response, FunctionUsage)>;

    async fn shutdown(&self) -> Result<()>;

    async fn add_functions(&self, functions: Vec<FunctionDefinition>) -> Result<()>;
    async fn remove_functions(&self, stack_id: StackID, names: Vec<String>) -> Result<()>;
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

#[derive(Clone)]
struct RuntimeImpl {
    mailbox: CallbackMailboxProcessor<MailboxMessage>,
}

struct CacheHashAndMemoryLimit {
    hash: wasmer_cache::Hash,
    memory_limit: KiloByte,
}

struct RuntimeState {
    function_provider: Box<dyn FunctionProvider>,
    hashkey_dict: HashMap<FunctionID, CacheHashAndMemoryLimit>,
    cache: FileSystemCache,
    database_service: Arc<DatabaseManager>,
}

impl RuntimeState {
    pub async fn new(
        function_provider: Box<dyn FunctionProvider>,
        cache_path: &Path,
        database_service: DatabaseManager,
    ) -> Result<Self> {
        let hashkey_dict = HashMap::new();
        let mut cache = FileSystemCache::new(cache_path).context("failed to create cache")?;
        cache.set_cache_extension(Some("wasmu"));

        Ok(Self {
            hashkey_dict,
            function_provider,
            cache,
            database_service: Arc::new(database_service),
        })
    }

    fn load_module(&mut self, function_id: &FunctionID) -> Result<(Store, Module)> {
        if self.hashkey_dict.contains_key(function_id) {
            let CacheHashAndMemoryLimit { hash, memory_limit } = self
                .hashkey_dict
                .get(function_id)
                .ok_or(Error::Internal("cache key can not be found"))?
                .to_owned();

            let store = create_store(*memory_limit);

            match unsafe { self.cache.load(&store, *hash) } {
                Ok(module) => Ok((store, module)),
                Err(e) => {
                    warn!("cached module is corrupted: {}", e);

                    let definition = self
                        .function_provider
                        .get(function_id)
                        .ok_or_else(|| Error::FunctionNotFound(function_id.clone()))?;

                    let module = Module::new(&store, definition.source.clone())?;

                    self.cache.store(*hash, &module)?;

                    Ok((store, module))
                }
            }
        } else {
            let function_definition = match self.function_provider.get(function_id) {
                Some(d) => d,
                None => {
                    return Err(Error::FunctionNotFound(function_id.clone())).map_err(Into::into);
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

            let store = create_store(function_definition.memory_limit);

            if let Ok(module) = Module::from_binary(&store, &function_definition.source) {
                if let Err(e) = self.cache.store(hash, &module) {
                    error!("failed to cache module: {e}, function id: {}", function_id);
                }
                Ok((store, module))
            } else {
                error!("can not build wasm module for function: {}", function_id);
                Err(Error::InvalidFunctionModule(function_id.clone())).map_err(Into::into)
            }
        }
    }

    async fn instantiate_function(&mut self, function_id: FunctionID) -> Result<Instance<Loaded>> {
        let definition = self
            .function_provider
            .get(&function_id)
            .ok_or_else(|| Error::FunctionNotFound(function_id.clone()))?
            .to_owned();

        let (store, module) = self.load_module(&function_id).unwrap();
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

    async fn add_functions(&self, functions: Vec<FunctionDefinition>) -> Result<()> {
        self.mailbox
            .post(MailboxMessage::AddFunctions(functions))
            .await
            .map_err(Into::into)
    }

    async fn remove_functions(&self, stack_id: StackID, names: Vec<String>) -> Result<()> {
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

pub async fn start(
    function_provider: Box<dyn FunctionProvider>,
    config: RuntimeConfig,
    db_service: DatabaseManager,
) -> Result<Box<dyn Runtime>> {
    let state = RuntimeState::new(function_provider, &config.cache_path, db_service).await?;
    let mailbox = CallbackMailboxProcessor::start(mailbox_step, state, 10000);
    Ok(Box::new(RuntimeImpl { mailbox }))
}

async fn mailbox_step(
    _mb: CallbackMailboxProcessor<MailboxMessage>,
    msg: MailboxMessage,
    mut state: RuntimeState,
) -> RuntimeState {
    //TODO: pass metering info to blockchain_manager service
    match msg {
        MailboxMessage::InvokeFunction(req) => {
            if let Ok(instance) = state.instantiate_function(req.function_id).await {
                tokio::spawn(async move {
                    let resp = tokio::task::spawn_blocking(move || {
                        instance
                            .start()
                            .context("can not run instance")
                            .and_then(|i| i.request(req.message))
                    })
                    .await
                    .unwrap(); // TODO: Handle spawn_blocking errors

                    match resp {
                        Ok(a) => req.reply.reply(a.await),
                        Err(e) => req.reply.reply(Err(e)),
                    };
                });
            } else {
                req.reply
                    .reply(Err(Error::Internal("Can not instantiate function")).map_err(Into::into))
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
