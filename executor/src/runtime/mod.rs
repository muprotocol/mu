//TODO
#![allow(dead_code)]
//TODO: Add logging

pub mod error;
pub mod function;
pub mod message;
pub mod providers;
pub mod types;

const MESSAGE_READ_BUF_CAP: usize = 8 * 1024;

use self::{
    function::{FunctionDefinition, FunctionID},
    message::{
        database::DbRequest,
        gateway::{GatewayRequest, GatewayResponse},
        FuncOutput, Message,
    },
    types::{FunctionIO, ID},
};
use crate::runtime::message::FuncInput;
use anyhow::{bail, Result};
use async_trait::async_trait;
use bytes::BufMut;
use std::{
    collections::{hash_map::Entry, HashMap},
    io::{BufRead, Write},
    sync::Arc,
};
use tokio::{sync::RwLock, task::JoinHandle};
use tokio_mailbox_processor::{callback::CallbackMailboxProcessor, ReplyChannel};

/// This is the FunctionProvider that should cache functions if needed.
#[async_trait]
pub trait FunctionProvider: Send {
    async fn get(&mut self, id: FunctionID) -> anyhow::Result<&FunctionDefinition>;
}

pub enum Request {
    Gateway {
        message: GatewayRequest,
        reply: ReplyChannel<Result<GatewayResponse>>,
    },
}

pub type RequestID = ID<Request>;
pub type InstanceID = ID<Instance>;

//TODO:
// * use metrics and MemoryUsage so we can report usage of memory and CPU time.
// * remove less frequently used source's from runtime
pub struct Runtime<P: FunctionProvider> {
    instances: HashMap<FunctionID, HashMap<InstanceID, Arc<RwLock<Instance>>>>,
    function_provider: P,
}

impl<P> Runtime<P>
where
    P: FunctionProvider + 'static,
{
    pub fn new(provider: P) -> Self {
        Self {
            instances: HashMap::new(),
            function_provider: provider,
        }
    }

    async fn get_instance(&mut self, function_id: FunctionID) -> Result<Arc<RwLock<Instance>>> {
        let instance_id = match self.instances.entry(function_id) {
            Entry::Vacant(v) => {
                let definition = self.function_provider.get(function_id).await?;
                let instance = Instance::new(definition).await?;
                let mut map = HashMap::new();
                let id = InstanceID::gen();
                map.insert(id, Arc::new(RwLock::new(instance)));
                v.insert(map);
                id
            }
            Entry::Occupied(mut o) => {
                let mut first_idle_instance = None;
                for (k, v) in o.get().iter() {
                    match v.try_read() {
                        Ok(i) if i.is_idle() => {
                            first_idle_instance = Some(k);
                            break;
                        }
                        _ => (),
                    }
                }

                match first_idle_instance {
                    None => {
                        let definition = self.function_provider.get(function_id).await?;
                        let instance = Instance::new(definition).await?;
                        let id = InstanceID::gen();
                        o.get_mut().insert(id, Arc::new(RwLock::new(instance)));
                        id
                    }
                    Some(k) => *k,
                }
            }
        };
        Ok(self
            .instances
            .get(&function_id)
            .unwrap()
            .get(&instance_id)
            .unwrap()
            .clone())
    }

    pub fn start(self) -> CallbackMailboxProcessor<Request> {
        let step = |msg: Request, mut runtime: Runtime<_>| async {
            match msg {
                Request::Gateway { message, reply } => {
                    if let Ok(instance) = runtime.get_instance(message.function_id).await {
                        let mut instance = instance.write_owned().await;
                        match tokio::task::spawn_blocking(move || instance.request(message)).await {
                            Err(e) => panic!("{e}"), //TODO: handle Error, and add log
                            Ok(r) => reply.reply(r),
                        };
                    }
                }
            }
            runtime
        };

        let mailbox = CallbackMailboxProcessor::start(step, self, 1000);

        mailbox
    }
}

#[derive(PartialEq)]
enum InstanceStatus {
    Idle,
    Busy,
}

pub struct Instance {
    function_id: FunctionID,
    join_handle: JoinHandle<()>,
    io: FunctionIO,
    state: InstanceStatus,
}

impl Instance {
    pub async fn new(definition: &FunctionDefinition) -> Result<Self> {
        let function = definition.create_function().await?;
        let (join_handle, pipes) = function.start()?;
        Ok(Self {
            function_id: definition.id(),
            io: FunctionIO::from_pipes(pipes),
            join_handle,
            state: InstanceStatus::Idle,
        })
    }

    pub fn is_finished(&self) -> bool {
        self.join_handle.is_finished()
    }

    pub fn is_idle(&self) -> bool {
        self.state == InstanceStatus::Idle
    }

    fn write_to_stdin(&mut self, input: Message) -> Result<()> {
        let mut bytes = input.as_bytes()?;
        bytes.put_u8(b'\n');
        self.io.stdin.write(&bytes)?; //TODO: check if all of buffer is written
        Ok(())
    }

    fn read_from_stdout(&mut self) -> Result<Message> {
        let mut buf = String::with_capacity(MESSAGE_READ_BUF_CAP);
        self.io.stdout.read_line(&mut buf)?; //TODO: check output and if it's 0, then pipe is
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
