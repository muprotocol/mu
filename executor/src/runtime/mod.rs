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
        database::DbRequest,
        gateway::{GatewayRequest, GatewayResponse},
        log::Log,
        FromMessage, Message,
    },
    types::{
        FunctionDefinition, FunctionHandle, FunctionID, FunctionProvider, FunctionUsage,
        InstanceID, InvokeFunctionRequest, Request,
    },
};
use crate::runtime::message::ToMessage;
use anyhow::{bail, Result};
use bytes::BufMut;
use futures::Future;
use mailbox_processor::callback::CallbackMailboxProcessor;
use std::{
    io::{BufRead, Write},
    time::Duration,
};
use wasmer_middlewares::metering::MeteringPoints;

//TODO:
// * use metrics and MemoryUsage so we can report usage of memory and CPU time.
// * remove less frequently used source's from runtime
pub struct Runtime {
    mailbox: CallbackMailboxProcessor<Request<'static>>,
}

struct RuntimeState {
    function_provider: Box<dyn FunctionProvider>,
}

impl RuntimeState {
    async fn instantiate_function(&mut self, function_id: FunctionID) -> Result<Instance> {
        let definition = self.function_provider.get(&function_id).await?;
        let instance = Instance::new(definition)?;
        Ok(instance)
    }
}

impl Runtime {
    pub fn start(function_provider: Box<dyn FunctionProvider>) -> Self {
        let state = RuntimeState { function_provider };

        let mailbox = CallbackMailboxProcessor::start(Self::mailbox_step, state, 10000);

        Self { mailbox }
    }

    pub async fn invoke_function(
        &self,
        function_id: FunctionID,
        message: GatewayRequest<'static>,
    ) -> Result<(GatewayResponse, FunctionUsage)> {
        let result = self
            .mailbox
            .post_and_reply(|r| {
                Request::InvokeFunction(InvokeFunctionRequest {
                    function_id,
                    message,
                    reply: r,
                })
            })
            .await;

        match result {
            Ok(r) => r,
            Err(e) => Err(e).map_err(Into::into),
        }
    }

    async fn mailbox_step<'a>(
        _mb: CallbackMailboxProcessor<Request<'a>>,
        msg: Request<'a>,
        mut runtime: RuntimeState,
    ) -> RuntimeState {
        //TODO: pass metering info to blockchain_manager service
        match msg {
            Request::InvokeFunction(req) => {
                if let Ok(instance) = runtime.instantiate_function(req.function_id).await {
                    tokio::spawn(async {
                        let resp =
                            tokio::task::spawn_blocking(move || instance.request(req.message))
                                .await
                                .unwrap(); // TODO: Handle spawn_blocking errors

                        match resp {
                            Ok(a) => req.reply.reply(a.await),
                            Err(a) => req.reply.reply(Err(a)),
                        };
                    });
                }
            }

            Request::Shutdown => {
                //TODO: find a way to kill running functions
            }
        }
        runtime
    }

    pub async fn shutdown(self) -> Result<()> {
        self.mailbox.post(Request::Shutdown).await?;
        self.mailbox.stop().await;
        Ok(())
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
        request: GatewayRequest,
    ) -> Result<impl Future<Output = Result<(GatewayResponse, FunctionUsage)>>> {
        //TODO: check function state
        self.write_to_stdin(request.to_message()?)?;
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

                    DbRequest::TYPE => (), //TODO

                    Log::TYPE => {
                        let log = Log::from_message(message)?;
                        println!("Log: {log:?}");
                    }
                    t => bail!("invalid message type: {t}"),
                },
                Err(e) => println!("Error while parsing resposne: {e:?}"),
            };
        }
    }
}
