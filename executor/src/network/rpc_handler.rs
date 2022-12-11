use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;

use crate::gateway;
use crate::network::connection_manager::{ConnectionID, ConnectionManager, RequestID};

#[async_trait]
pub trait RpcHandler: Clone {
    async fn request_received(
        &self,
        connection_id: ConnectionID,
        request_id: RequestID,
        request_data: Bytes,
    );
}

pub enum RpcRequest {
    ExecuteFunctionRequest(
        gateway::Request<'static>,
        Box<dyn FnOnce(&dyn ConnectionManager, gateway::Response)>,
    ),
}

// Take an Fn(RpcRequest) -> Future, invoke per request, spawn new task to handle
