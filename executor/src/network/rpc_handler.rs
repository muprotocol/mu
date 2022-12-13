mod protos;

use std::pin::Pin;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use bytes::Bytes;
use dyn_clonable::clonable;
use futures::Future;
use log::warn;
use protobuf::Message;

use crate::gateway;
use crate::network::connection_manager::{ConnectionID, ConnectionManager, RequestID};

// TODO: bad design, we receive the request from glue code, but access the
// connection manager directly to send requests and replies. Should use one
// approach across entire code.
#[async_trait]
#[clonable]
pub trait RpcHandler: Send + Sync + Clone {
    fn request_received(
        &self,
        connection_id: ConnectionID,
        request_id: RequestID,
        request_data: Bytes,
    );

    async fn send_execute_function<'a>(
        &self,
        connection_id: ConnectionID,
        request: gateway::Request<'a>,
    ) -> Result<gateway::Response>;
}

#[async_trait]
#[clonable]
pub trait RpcRequestHandler: Clone {
    async fn handle_request(&self, request: RpcRequest) -> ();
}

#[allow(clippy::type_complexity)]
pub enum RpcRequest {
    ExecuteFunctionRequest(
        gateway::Request<'static>,
        Box<
            dyn FnOnce(
                    Result<gateway::Response>,
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
#[async_trait]
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
                    let request = gateway::Request::<'static>::try_from(request)
                        .context("Execute function request contains invalid data")?;

                    let connection_manager = self.connection_manager.clone();
                    let rpc_request = RpcRequest::ExecuteFunctionRequest(
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

    async fn send_execute_function<'a>(
        &self,
        connection_id: ConnectionID,
        request: gateway::Request<'a>,
    ) -> Result<gateway::Response> {
        let request = protos::rpc::Request::from(request);
        let request = protos::rpc::RpcRequest {
            request: Some(protos::rpc::rpc_request::Request::ExecuteFunction(request)),
            ..Default::default()
        };
        let request = request
            .write_to_bytes()
            .context("Failed to serialize execute function request")?;
        let reply = self
            .connection_manager
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
                let response = gateway::Response::try_from(response)
                    .context("Failed to read execute function response")?;
                Ok(response)
            }
        }
    }
}

async fn send_execute_function_reply(
    connection_manager: Box<dyn ConnectionManager>,
    response: Result<gateway::Response>,
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
