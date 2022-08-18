//TODO
#![allow(dead_code)]
//TODO: Add logging

pub mod error;
mod function;
mod message;
mod providers;
pub mod types;

use self::{
    function::{FunctionDefinition, FunctionID, FunctionIO},
    message::gateway::{GatewayRequest, GatewayResponse},
    types::ID,
};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::task::JoinHandle;
use tokio_mailbox_processor::{callback::CallbackMailboxProcessor, ReplyChannel};

/// This is the FunctionProvider that should cache functions if needed.
#[async_trait]
pub trait FunctionProvider {
    async fn get(&mut self, id: FunctionID) -> anyhow::Result<&FunctionDefinition>;
}

pub enum Request {
    Gateway {
        message: GatewayRequest,
        reply: ReplyChannel<GatewayResponse>,
    },
}

pub type RequestID = ID<Request>;

//TODO:
// * use metrics and MemoryUsage so we can report usage of memory and CPU time.
// * remove less frequently used source's from runtime
// * hold more than one instance of functions and load balance on them
pub struct Runtime<P: FunctionProvider> {
    instances: HashMap<FunctionID, Instance>,
    pending_requests: HashMap<RequestID, ReplyChannel<GatewayResponse>>,
    function_provider: P,
}

impl<P> Runtime<P>
where
    P: FunctionProvider,
{
    pub fn new(provider: P) -> Self {
        Self {
            instances: HashMap::new(),
            function_provider: provider,
            pending_requests: HashMap::new(),
        }
    }

    //TODO: check and maintain function status better
    async fn run_function(&mut self, id: FunctionID) -> Result<()> {
        match self.instances.get(&id) {
            Some(i) if !i.is_finished() => (),
            _ => {
                let definition = self.function_provider.get(id).await?;
                let instance = Instance::new(definition).await?;
                self.instances.insert(id, instance);
            }
        }
        Ok(())
    }

    pub async fn start(&mut self) -> CallbackMailboxProcessor<Request> {
        async fn step(msg: Request, _state: ()) -> () {
            match msg {
                Request::Gateway { message, reply } => {
                    todo!()
                }
            }
        }

        let mailbox = CallbackMailboxProcessor::start(step, (), 1000);

        mailbox
    }
}

struct Instance {
    id: FunctionID,
    io: FunctionIO,
    join_handle: JoinHandle<()>,
}

impl Instance {
    pub async fn new(definition: &FunctionDefinition) -> Result<Self> {
        let function = definition.create_function().await?;
        let (join_handle, io) = function.start()?;
        Ok(Self {
            id: definition.id,
            io,
            join_handle,
        })
    }

    pub fn is_finished(&self) -> bool {
        self.join_handle.is_finished()
    }
}
