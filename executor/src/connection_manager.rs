//TODO
#![allow(dead_code)]

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use bytes::Bytes;
use futures::{FutureExt, StreamExt};
use log::*;
use quinn::{Connecting, Endpoint, Incoming, NewConnection, RecvStream, SendStream, ServerConfig};
use tokio_mailbox_processor::{
    plain::{MessageReceiver, PlainMailboxProcessor},
    ReplyChannel,
};

pub type ConnectionID = u32;

#[async_trait]
pub trait ConnectionManagerCallbacks: Send + Sync + 'static {
    async fn new_connection_available(&self, id: ConnectionID);
    async fn connection_closed(&self, id: ConnectionID);
    async fn datagram_received(&self, id: ConnectionID, data: Bytes);
    async fn req_rep_received(&self, id: ConnectionID, data: Bytes) -> Bytes;
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
    SendDatagram(ConnectionID, Bytes, ReplyChannel<()>),
    SendReqRep(ConnectionID, Bytes, ReplyChannel<Bytes>),
    Disconnect(ConnectionID, ReplyChannel<()>),
}

struct ConnectionManagerImpl {
    mailbox: PlainMailboxProcessor<ConnectionManagerMessage>,
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
            .post_and_reply(|r| ConnectionManagerMessage::SendDatagram(id, data, r))
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
            .post_and_reply(|r| ConnectionManagerMessage::Disconnect(id, r))
            .await
            .map_err(Into::into)
    }
}

pub async fn start(
    listen_address: IpAddr,
    listen_port: u16,
    callbacks: Box<dyn ConnectionManagerCallbacks>,
) -> Result<Box<dyn ConnectionManager>> {
    // TODO: make self-signed certificates optional
    let (private_key, cert_chain) = make_self_signed_certificate();
    let server_config = ServerConfig::with_single_cert(cert_chain, private_key)
        .context("Failed to configure server")?;

    let (endpoint, incoming) =
        Endpoint::server(server_config, SocketAddr::new(listen_address, listen_port))
            .context("Failed to start server")?;

    let mailbox = PlainMailboxProcessor::start(|r| body(r, callbacks, endpoint, incoming), 10000);

    Ok(Box::new(ConnectionManagerImpl { mailbox }))
}

fn make_self_signed_certificate() -> (rustls::PrivateKey, Vec<rustls::Certificate>) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = cert.serialize_der().unwrap();
    let priv_key = cert.serialize_private_key_der();
    println!("{}", priv_key.len());
    let priv_key = rustls::PrivateKey(priv_key);
    let cert_chain = vec![rustls::Certificate(cert_der.clone())];
    (priv_key, cert_chain)
}

type OpenConnection = (NewConnection, SendStream, RecvStream);
type ConnectionMap = HashMap<ConnectionID, OpenConnection>;

async fn body(
    mut message_receiver: MessageReceiver<ConnectionManagerMessage>,
    callbacks: Box<dyn ConnectionManagerCallbacks>,
    mut endpoint: Endpoint,
    mut incoming: Incoming,
) {
    let mut next_connection_id = 0;
    let mut connections: ConnectionMap = HashMap::new();
    'main_loop: loop {
        tokio::select! {
            msg = message_receiver.receive() => {
                match msg {
                    None => {
                        warn!("Mailbox was stopped, stopping connection manager");
                        break 'main_loop;
                    }

                    Some(ConnectionManagerMessage::Connect(ip, port, rep)) => {

                    }

                    Some(ConnectionManagerMessage::SendDatagram(id, bytes, rep)) => {

                    }

                    Some(ConnectionManagerMessage::SendReqRep(id, bytes, rep)) => {

                    }

                    Some(ConnectionManagerMessage::Disconnect(id, rep)) => {

                    }
                }
            }

            maybe_connecting = incoming.next() => {
                if !process_incoming(&mut next_connection_id, &mut connections, callbacks.as_ref(), maybe_connecting).await {
                    warn!("Local endpoint was stopped, stopping connection manager");
                    break 'main_loop;
                }
            }

            message = get_next_message(&mut connections).fuse() => {
                process_message(message, callbacks.as_ref(), &mut connections);
            }
        };
    }
}

async fn process_incoming(
    next_connection_id: &mut u32,
    connections: &mut ConnectionMap,
    callbacks: &dyn ConnectionManagerCallbacks,
    maybe_connecting: Option<Connecting>,
) -> bool {
    let connecting = match maybe_connecting {
        None => return false,
        Some(x) => x,
    };

    let mut new_connection = match connecting.await {
        Ok(x) => x,
        Err(f) => {
            info!("Failed to accept connection due to: {}", f);
            return true;
        }
    };

    let id = *next_connection_id;
    *next_connection_id += 1;

    trace!(
        "New connection available from {}, assigning id {}",
        new_connection.connection.remote_address(),
        id
    );

    let (send, recv) = match new_connection.bi_streams.next().await {
        None => return false,
        Some(Ok(x)) => x,
        Some(Err(f)) => {
            info!(
                "Failed to accept bi-directional stream from {}, won't connect",
                id
            );
            return true;
        }
    };

    connections.insert(id, (new_connection, send, recv));
    callbacks.new_connection_available(id).await;

    return true;
}

enum IncomingMessage {
    Datagram(ConnectionID, Bytes),
    ReqRep(ConnectionID, Bytes),
}

async fn get_next_message(connections: &mut ConnectionMap) -> IncomingMessage {
    todo!();
}

async fn process_message(
    message: IncomingMessage,
    callbacks: &dyn ConnectionManagerCallbacks,
    connections: &mut ConnectionMap,
) -> Result<()> {
    match message {
        IncomingMessage::Datagram(id, bytes) => {
            callbacks.datagram_received(id, bytes.clone()).await
        }

        IncomingMessage::ReqRep(id, bytes) => {
            let (_, send, _) = connections.get_mut(&id).unwrap();
            // TODO: handle errors
            let result = callbacks.req_rep_received(id, bytes).await;
            send.write_all(result.as_ref()).await?;
        }
    }

    Ok(())
}
