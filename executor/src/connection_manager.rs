use std::net::IpAddr;

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use quinn::Endpoint;
use tokio_mailbox_processor::{callback::CallbackMailboxProcessor, ReplyChannel};

pub type ConnectionID = u32;

#[async_trait]
pub trait ConnectionManagerCallbacks: Send + 'static {
    async fn new_connection_available(&self, id: ConnectionID);
    async fn connection_closed(&self, id: ConnectionID);
    async fn datagram_received(&self, data: Bytes);
    async fn req_rep_received(&self, data: Bytes) -> Bytes;
}

#[async_trait]
pub trait ConnectionManager {
    async fn connect(&self, address: IpAddr, port: u16) -> Result<ConnectionID>;
    async fn send_datagram(&self, id: ConnectionID, data: Bytes) -> Result<()>;
    async fn send_req_rep(&self, id: ConnectionID, data: Bytes) -> Result<Bytes>;
    async fn disconnect(&self, id: ConnectionID) -> Result<()>;
}

enum ConnectionManagerMessage {
    Connect(IpAddr, u16, ReplyChannel<ConnectionID>),
    SendDatagram(ConnectionID, Bytes),
    SendReqRep(ConnectionID, Bytes, ReplyChannel<Bytes>),
    Disconnect(ConnectionID),
}

struct ConnectionManagerImpl {
    mailbox: CallbackMailboxProcessor<ConnectionManagerMessage>,
}

#[async_trait]
impl ConnectionManager for ConnectionManagerImpl {
    async fn connect(&self, address: IpAddr, port: u16) -> Result<ConnectionID> {
        self.mailbox
            .post_and_reply(|r| ConnectionManagerMessage::Connect(address, port, r))
            .await
            .map_err(Into::into)
    }

    async fn send_datagram(&self, id: ConnectionID, data: Bytes) -> Result<()> {
        self.mailbox
            .post(ConnectionManagerMessage::SendDatagram(id, data))
            .await
            .map_err(Into::into)
    }

    async fn send_req_rep(&self, id: ConnectionID, data: Bytes) -> Result<Bytes> {
        self.mailbox
            .post_and_reply(|r| ConnectionManagerMessage::SendReqRep(id, data, r))
            .await
            .map_err(Into::into)
    }

    async fn disconnect(&self, id: ConnectionID) -> Result<()> {
        self.mailbox
            .post(ConnectionManagerMessage::Disconnect(id))
            .await
            .map_err(Into::into)
    }
}

struct ConnectionManagerState {
    endpoint: Endpoint,
    callbacks: Box<dyn ConnectionManagerCallbacks>,
}

pub async fn start(
    listen_address: IpAddr,
    listen_port: u16,
    callbacks: Box<dyn ConnectionManagerCallbacks>,
) -> Result<Box<dyn ConnectionManager>> {
    let endpoint = todo!();

    let state = ConnectionManagerState {
        endpoint,
        callbacks,
    };
    let mailbox = CallbackMailboxProcessor::start(step, state, 10000);

    Ok(Box::new(ConnectionManagerImpl { mailbox }))
}

async fn step(
    msg: ConnectionManagerMessage,
    state: ConnectionManagerState,
) -> ConnectionManagerState {
    todo!()
}
