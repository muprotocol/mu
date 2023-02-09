mod protos;

use std::pin::Pin;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use bytes::Bytes;
use dyn_clonable::clonable;
use futures::Future;
use log::warn;
use mu_stack::FunctionID;
use musdk_common::{Request, Response};
use protobuf::{Message, MessageField};

use super::{
    connection_manager::{ConnectionManager, RequestID},
    ConnectionID,
};

#[clonable]
pub trait RpcHandler: Send + Sync + Clone {
    fn request_received(
        &self,
        connection_id: ConnectionID,
        request_id: RequestID,
        request_data: Bytes,
    );

    // To the best of my knowledge, the future from an async method has the same
    // lifetime as the self parameter, which we don't want here, so we return
    // a separately constructed future.
    fn send_execute_function<'a>(
        &self,
        connection_id: ConnectionID,
        function_id: FunctionID,
        request: Request<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<Response<'static>>> + Send + 'a>>;
}

#[async_trait]
#[clonable]
pub trait RpcRequestHandler: Clone {
    async fn handle_request(&self, request: RpcRequest) -> ();
}

#[allow(clippy::type_complexity)]
pub enum RpcRequest {
    ExecuteFunctionRequest(
        FunctionID,
        Request<'static>,
        Box<
            dyn FnOnce(
                    Result<Response<'static>>,
                ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>
                + Send
                + Sync,
        >,
    ),
}

#[derive(Clone)]
struct RpcHandlerImpl<RequestHandler: RpcRequestHandler + Clone + Send + Sync> {
    request_handler: RequestHandler,
    connection_manager: Box<dyn ConnectionManager>,
}

pub fn new(
    connection_manager: Box<dyn ConnectionManager>,
    request_handler: impl RpcRequestHandler + Clone + Send + Sync + 'static,
) -> Box<dyn RpcHandler> {
    Box::new(RpcHandlerImpl {
        request_handler,
        connection_manager,
    })
}

// TODO: implement validation when deserializing messages from network
impl<RequestHandler: RpcRequestHandler + Clone + Send + Sync + 'static> RpcHandler
    for RpcHandlerImpl<RequestHandler>
{
    fn request_received(
        &self,
        connection_id: ConnectionID,
        request_id: RequestID,
        request_data: Bytes,
    ) {
        let helper = move || {
            let rpc_request = protos::rpc::RpcRequest::parse_from_bytes(&request_data)
                .context("Failed to parse request data")?;

            match rpc_request.request {
                None => bail!("Received empty request"),
                Some(protos::rpc::rpc_request::Request::ExecuteFunction(request)) => {
                    let Some(function_id) = request.function_id.0 else {
                        bail!("Empty function ID in execute function request");
                    };
                    let function_id = FunctionID::try_from(*function_id)
                        .context("Failed to read function ID from execute function request")?;

                    let Some(request) = request.request.0 else {
                        bail!("Empty request in execute function request");
                    };
                    let request = Request::<'static>::try_from(*request)
                        .context("Execute function request contains invalid data")?;

                    let connection_manager = self.connection_manager.clone();
                    let rpc_request = RpcRequest::ExecuteFunctionRequest(
                        function_id,
                        request,
                        Box::new(move |response| {
                            Box::pin(send_execute_function_reply(
                                connection_manager,
                                response,
                                connection_id,
                                request_id,
                            ))
                        }),
                    );

                    let request_handler = self.request_handler.clone();
                    tokio::spawn(async move {
                        request_handler.handle_request(rpc_request).await;
                    });
                }
            }

            Ok(())
        };

        if let Err(f) = helper() {
            warn!("Failed to process RPC call: {f:?}");
        }
    }

    fn send_execute_function<'a>(
        &self,
        connection_id: ConnectionID,
        function_id: FunctionID,
        request: Request<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<Response<'static>>> + Send + 'a>> {
        let connection_manager = self.connection_manager.clone();
        Box::pin(async move {
            let function_id = protos::rpc::FunctionID::from(function_id);
            let request = protos::rpc::Request::from(request);
            let request = protos::rpc::ExecuteFunctionRequest {
                request: MessageField(Some(Box::new(request))),
                function_id: MessageField(Some(Box::new(function_id))),
                ..Default::default()
            };
            let request = protos::rpc::RpcRequest {
                request: Some(protos::rpc::rpc_request::Request::ExecuteFunction(request)),
                ..Default::default()
            };
            let request = request
                .write_to_bytes()
                .context("Failed to serialize execute function request")?;
            let reply = connection_manager
                .send_req_rep(connection_id, request.into())
                .await
                .context("Failed to send execute function request")?;
            let response = protos::rpc::ExecuteFunctionResponse::parse_from_bytes(&reply)
                .context("Failed to deserialize execute function response")?;
            match response.result {
                None => bail!("Received empty response to execute function request"),
                Some(protos::rpc::execute_function_response::Result::Error(f)) => {
                    bail!("Received error response to execute function request: {f}")
                }
                Some(protos::rpc::execute_function_response::Result::Ok(response)) => {
                    let response = Response::<'static>::try_from(response)
                        .context("Failed to read execute function response")?;
                    Ok(response)
                }
            }
        })
    }
}

async fn send_execute_function_reply(
    connection_manager: Box<dyn ConnectionManager>,
    response: Result<Response<'static>>,
    connection_id: ConnectionID,
    request_id: RequestID,
) {
    let helper = async move {
        let response = match response {
            Ok(response) => protos::rpc::ExecuteFunctionResponse {
                result: Some(protos::rpc::execute_function_response::Result::Ok(
                    protos::rpc::Response::from(response),
                )),
                ..Default::default()
            },
            Err(f) => protos::rpc::ExecuteFunctionResponse {
                result: Some(protos::rpc::execute_function_response::Result::Error(
                    format!("{f:?}"),
                )),
                ..Default::default()
            },
        };
        let response_data = response
            .write_to_bytes()
            .context("Failed to serialize execute function response data")?;
        connection_manager
            .send_reply(connection_id, request_id, response_data.into())
            .await?;
        anyhow::Ok(())
    };

    if let Err(f) = helper.await {
        warn!("Failed to send execute function reply: {f:?}");
    }
}
