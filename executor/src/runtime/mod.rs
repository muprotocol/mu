//TODO
#![allow(dead_code)]
//TODO: Add logging

pub mod error;
pub mod function;
pub mod message;
pub mod providers;

const MESSAGE_READ_BUF_CAP: usize = 8 * 1024;

use self::{
    function::{FunctionDefinition, FunctionHandle, FunctionID},
    message::{
        database::DbRequest,
        gateway::{GatewayRequest, GatewayResponse},
        FromMessage, Message,
    },
};
use crate::runtime::message::ToMessage;
use anyhow::{bail, Result};
use async_trait::async_trait;
use bytes::BufMut;
use mailbox_processor::{callback::CallbackMailboxProcessor, ReplyChannel};
use std::{
    collections::{hash_map::Entry, HashMap},
    io::{BufRead, Write},
    sync::Arc,
};
use tokio::sync::RwLock;
use uuid::Uuid;

/// This is the FunctionProvider that should cache functions if needed.
#[async_trait]
pub trait FunctionProvider: Send {
    async fn get(&mut self, id: &FunctionID) -> anyhow::Result<&FunctionDefinition>;
}

pub enum Request {
    InvokeFunction(InvokeFunctionRequest),
}

pub struct InvokeFunctionRequest {
    // TODO: not needed in public interface
    pub function_id: FunctionID,
    pub message: GatewayRequest,
    pub reply: ReplyChannel<Result<GatewayResponse>>,
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
pub struct InstanceID(Uuid);

impl InstanceID {
    fn generate_random() -> Self {
        Self(Uuid::new_v4())
    }
}

//TODO:
// * use metrics and MemoryUsage so we can report usage of memory and CPU time.
// * remove less frequently used source's from runtime
pub struct Runtime {
    // TODO: make mailbox private, implement methods for posting messages
    pub mailbox: CallbackMailboxProcessor<Request>,
}

struct RuntimeState {
    instances: HashMap<FunctionID, HashMap<InstanceID, Arc<RwLock<Instance>>>>,
    function_provider: Box<dyn FunctionProvider>,
}

impl Runtime {
    pub fn start(function_provider: Box<dyn FunctionProvider>) -> Self {
        let state = RuntimeState {
            instances: HashMap::new(),
            function_provider,
        };

        let mailbox = CallbackMailboxProcessor::start(Self::mailbox_step, state, 10000);

        Self { mailbox }
    }

    async fn mailbox_step(
        _: CallbackMailboxProcessor<Request>,
        msg: Request,
        mut runtime: RuntimeState,
    ) -> RuntimeState {
        match msg {
            Request::InvokeFunction(req) => {
                if let Ok(instance) = Self::get_instance(&mut runtime, req.function_id).await {
                    let mut instance = instance.write_owned().await;
                    // TODO: Handle spawn_blocking errors
                    // TODO: Handle node shutdown, how can we cancel running functions?
                    tokio::task::spawn_blocking(move || {
                        req.reply.reply(instance.request(req.message))
                    });
                }
            }
        }
        runtime
    }

    async fn get_instance(
        state: &mut RuntimeState,
        function_id: FunctionID,
    ) -> Result<Arc<RwLock<Instance>>> {
        let instance_id = match state.instances.entry(function_id.clone()) {
            Entry::Vacant(v) => {
                let definition = state.function_provider.get(&function_id).await?;
                let instance = Instance::new(definition).await?;
                let mut map = HashMap::new();
                let id = InstanceID::generate_random();
                map.insert(id, Arc::new(RwLock::new(instance)));
                v.insert(map);
                id
            }

            Entry::Occupied(mut o) => {
                let first_idle_instance = o
                    .get()
                    .iter()
                    .filter_map(|(k, v)| match v.try_read() {
                        Ok(i) if i.is_idle() => Some(k),
                        _ => None,
                    })
                    .nth(0);

                match first_idle_instance {
                    None => {
                        let definition = state.function_provider.get(&function_id).await?;
                        let instance = Instance::new(definition).await?;
                        let id = InstanceID::generate_random();
                        o.get_mut().insert(id, Arc::new(RwLock::new(instance)));
                        id
                    }
                    Some(k) => *k,
                }
            }
        };
        Ok(state
            .instances
            .get(&function_id)
            .unwrap()
            .get(&instance_id)
            .unwrap()
            .clone())
    }
}

#[derive(PartialEq)]
enum InstanceStatus {
    Idle,
    Busy,
}

pub struct Instance {
    function_id: FunctionID,
    handle: FunctionHandle,
    state: InstanceStatus,
}

impl Instance {
    pub async fn new(definition: &FunctionDefinition) -> Result<Self> {
        let handle = function::start(definition)?;
        Ok(Self {
            function_id: definition.id.clone(),
            handle,
            state: InstanceStatus::Idle,
        })
    }

    pub fn is_finished(&self) -> bool {
        self.handle.join_handle.is_finished()
    }

    pub fn is_idle(&self) -> bool {
        self.state == InstanceStatus::Idle
    }

    fn write_to_stdin(&mut self, input: Message) -> Result<()> {
        let mut bytes = input.as_bytes()?;
        bytes.put_u8(b'\n');
        self.handle.io.stdin.write(&bytes)?; //TODO: check if all of buffer is written
        Ok(())
    }

    fn read_from_stdout(&mut self) -> Result<Message> {
        let mut buf = String::with_capacity(MESSAGE_READ_BUF_CAP);
        self.handle.io.stdout.read_line(&mut buf)?; //TODO: check output and if it's 0, then pipe is
                                                    //closed
        println!("Message read: `{buf}`");
        serde_json::from_slice(buf.as_bytes()).map_err(Into::into)
    }

    pub fn request(&mut self, request: GatewayRequest) -> Result<GatewayResponse> {
        self.state = InstanceStatus::Busy;
        //TODO: check function state
        self.write_to_stdin(request.to_message()?)?;
        loop {
            let message = self.read_from_stdout();
            match message {
                Ok(message) => match message.r#type.as_str() {
                    GatewayResponse::TYPE => {
                        self.state = InstanceStatus::Idle;
                        return GatewayResponse::from_message(message);
                    }
                    DbRequest::TYPE => todo!(),
                    t => bail!("invalid message type: {t}"),
                },
                Err(e) => println!("Error while parsing resposne: {e:?}"),
            };
        }
    }
}
