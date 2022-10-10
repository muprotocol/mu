//TODO
#![allow(dead_code)]
//TODO: Add logging

pub mod error;
pub mod function;
pub mod instance;
pub mod message;
pub mod providers;
pub mod types;

use anyhow::{Context, Result};
use async_trait::async_trait;
use dyn_clonable::clonable;
use log::*;
use mailbox_processor::{callback::CallbackMailboxProcessor, ReplyChannel};
use mu_stack::StackID;
use std::{collections::HashMap, path::Path};
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
use crate::{gateway, runtime::message::ToMessage};

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

//TODO:
// * use metrics and MemoryUsage so we can report usage of memory and CPU time.
#[derive(Clone)]
struct RuntimeImpl {
    mailbox: CallbackMailboxProcessor<MailboxMessage>,
}

struct RuntimeState {
    function_provider: Box<dyn FunctionProvider>,
    hashkey_dict: HashMap<FunctionID, wasmer_cache::Hash>,
    cache: FileSystemCache,
    store: Store, // We only need this store for it's configuration
}

impl RuntimeState {
    pub fn new(function_provider: Box<dyn FunctionProvider>, cache_path: &Path) -> Result<Self> {
        let mut cache = FileSystemCache::new(cache_path).context("failed to create cache")?;
        cache.set_cache_extension(Some("wasmu"));

        Ok(Self {
            hashkey_dict: HashMap::new(),
            function_provider,
            cache,
            store: create_store(),
        })
    }

    fn load_module(&mut self, function_id: &FunctionID) -> Result<Module> {
        let key = self
            .hashkey_dict
            .get(function_id)
            .ok_or(Error::Internal("cache key can not be found"))?
            .to_owned();

        match unsafe { self.cache.load(&self.store, key) } {
            Ok(module) => Ok(module),
            Err(e) => {
                warn!("cached module is corrupted: {}", e);

                let definition = self
                    .function_provider
                    .get(function_id)
                    .ok_or_else(|| Error::FunctionNotFound(function_id.clone()))?;

                let module = Module::new(&self.store, definition.source.clone())?; //TODO: This clone should be removed once we merged PR #39

                self.cache.store(key, &module)?;
                Ok(module)
            }
        }
    }

    async fn instantiate_function(&mut self, function_id: FunctionID) -> Result<Instance<Loaded>> {
        let definition = self
            .function_provider
            .get(&function_id)
            .ok_or_else(|| Error::FunctionNotFound(function_id.clone()))?;
        let instance = Instance::new(function_id.clone(), definition.envs.clone());
        let module = self.load_module(&function_id)?;
        Ok(instance.load_module(module))
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

pub fn start(
    function_provider: Box<dyn FunctionProvider>,
    config: RuntimeConfig,
) -> Result<Box<dyn Runtime>> {
    let state = RuntimeState::new(function_provider, &config.cache_path)?;
    let mailbox = CallbackMailboxProcessor::start(mailbox_step, state, 10000);
    Ok(Box::new(RuntimeImpl { mailbox }))
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
                tokio::spawn(async move {
                    let resp = tokio::task::spawn_blocking(move || {
                        let instance = instance.start().context("can not run instance").unwrap(); //TODO: Handle Errors
                        instance.request(req.message)
                    })
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
                if runtime.hashkey_dict.contains_key(&f.id) {
                    continue;
                }

                let mut hash_array = Vec::with_capacity(f.id.function_name.len() + 16); // Uuid is 16 bytes
                hash_array.extend_from_slice(f.id.stack_id.get_bytes());
                hash_array.extend_from_slice(f.id.function_name.as_bytes());
                let hash = wasmer_cache::Hash::generate(&hash_array);
                runtime.hashkey_dict.insert(f.id.clone(), hash);

                if let Ok(module) = Module::from_binary(&runtime.store, &f.source) {
                    if let Err(e) = runtime.cache.store(hash, &module) {
                        error!("failed to cache module: {e}, function id: {}", f.id);
                    }
                    runtime.function_provider.add_function(f);
                } else {
                    error!("can not build wasm module for function: {}", f.id);
                }
            }
        }

        MailboxMessage::RemoveFunctions(stack_id, functions_names) => {
            for function_name in functions_names {
                let function_id = FunctionID {
                    stack_id,
                    function_name,
                };

                runtime.function_provider.remove_function(&function_id);
                runtime.hashkey_dict.remove(&function_id);
            }
        }

        MailboxMessage::GetFunctionNames(stack_id, r) => {
            r.reply(runtime.function_provider.get_function_names(&stack_id));
        }
    }
    runtime
}
