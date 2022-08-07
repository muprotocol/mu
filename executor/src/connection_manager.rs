// TODO list:
// * don't raise callbacks on the mailbox's task
// * separate out the send and receive channels between each pair of peers
// * don't fail the entire mailbox when a connect() request fails

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use bytes::Bytes;
use futures::{future, FutureExt, SinkExt, StreamExt};
use log::*;
use quinn::{
    ClientConfig, Connecting, Endpoint, Incoming, NewConnection, RecvStream, SendStream,
    ServerConfig,
};
use tokio_mailbox_processor::{
    plain::{MessageReceiver, PlainMailboxProcessor},
    ReplyChannel,
};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

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
    async fn stop(&self) -> Result<()>;
}

enum ConnectionManagerMessage {
    Connect(IpAddr, u16, ReplyChannel<Result<ConnectionID>>),
    SendDatagram(ConnectionID, Bytes, ReplyChannel<Result<()>>),
    SendReqRep(ConnectionID, Bytes, ReplyChannel<Result<Bytes>>),
    Disconnect(ConnectionID, ReplyChannel<()>),
    Stop(ReplyChannel<()>),
}

struct ConnectionManagerImpl {
    mailbox: PlainMailboxProcessor<ConnectionManagerMessage>,
}

#[async_trait]
impl ConnectionManager for ConnectionManagerImpl {
    async fn connect(&self, address: IpAddr, port: u16) -> Result<ConnectionID> {
        flatten_and_map_result(
            self.mailbox
                .post_and_reply(|r| ConnectionManagerMessage::Connect(address, port, r))
                .await,
        )
    }

    async fn send_datagram(&self, id: ConnectionID, data: Bytes) -> Result<()> {
        flatten_and_map_result(
            self.mailbox
                .post_and_reply(|r| ConnectionManagerMessage::SendDatagram(id, data, r))
                .await,
        )
    }

    async fn send_req_rep(&self, id: ConnectionID, data: Bytes) -> Result<Bytes> {
        flatten_and_map_result(
            self.mailbox
                .post_and_reply(|r| ConnectionManagerMessage::SendReqRep(id, data, r))
                .await,
        )
    }

    async fn disconnect(&self, id: ConnectionID) -> Result<()> {
        self.mailbox
            .post_and_reply(|r| ConnectionManagerMessage::Disconnect(id, r))
            .await
            .map_err(Into::into)
    }

    async fn stop(&self) -> Result<()> {
        self.mailbox
            .post_and_reply(|r| ConnectionManagerMessage::Stop(r))
            .await
            .map_err(Into::into)
    }
}

fn flatten_and_map_result<T>(r: Result<Result<T>, tokio_mailbox_processor::Error>) -> Result<T> {
    match r {
        Ok(Ok(x)) => Ok(x),
        Ok(Err(f)) => Err(f),
        Err(f) => Err(f.into()),
    }
}

pub async fn start(
    listen_address: IpAddr,
    listen_port: u16,
    callbacks: Box<dyn ConnectionManagerCallbacks>,
) -> Result<Box<dyn ConnectionManager>> {
    info!(
        "Starting connection manager on {}:{}",
        listen_address, listen_port
    );

    // TODO: make self-signed certificates optional
    let (private_key, cert_chain) = make_self_signed_certificate();
    let server_config = ServerConfig::with_single_cert(cert_chain, private_key)
        .context("Failed to configure server")?;

    let (mut endpoint, incoming) =
        Endpoint::server(server_config, SocketAddr::new(listen_address, listen_port))
            .context("Failed to start server")?;

    let client_config = make_client_configuration();
    endpoint.set_default_client_config(client_config);

    let mailbox =
        PlainMailboxProcessor::start(|mb, r| body(mb, r, callbacks, endpoint, incoming), 10000);

    Ok(Box::new(ConnectionManagerImpl { mailbox }))
}

fn make_self_signed_certificate() -> (rustls::PrivateKey, Vec<rustls::Certificate>) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = cert.serialize_der().unwrap();
    let priv_key = cert.serialize_private_key_der();
    let priv_key = rustls::PrivateKey(priv_key);
    let cert_chain = vec![rustls::Certificate(cert_der)];
    (priv_key, cert_chain)
}

fn make_client_configuration() -> ClientConfig {
    let crypto = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_custom_certificate_verifier(SkipServerVerification::new())
        .with_no_client_auth();

    ClientConfig::new(Arc::new(crypto))
}

type OpenConnection = (
    NewConnection,
    FramedRead<RecvStream, LengthDelimitedCodec>,
    FramedWrite<SendStream, LengthDelimitedCodec>,
);
type ConnectionMap = HashMap<ConnectionID, OpenConnection>;

async fn body(
    _mailbox: PlainMailboxProcessor<ConnectionManagerMessage>,
    mut message_receiver: MessageReceiver<ConnectionManagerMessage>,
    callbacks: Box<dyn ConnectionManagerCallbacks>,
    mut endpoint: Endpoint,
    mut incoming: Incoming,
) {
    let mut next_connection_id = 0;
    let mut connections: ConnectionMap = HashMap::new();
    let mut stop_reply_channel = None;

    // TODO: this code is not async enough. For example, if connecting to
    // a peer takes a long time, incoming messages won't be processed until
    // it's done.
    'main_loop: loop {
        // TODO select! requires all futures to be cancellation-safe. Are they?
        tokio::select! {
            msg = message_receiver.receive() => {
                match msg {
                    None => {
                        warn!("Mailbox was stopped, stopping connection manager");
                        break 'main_loop;
                    }

                    Some(ConnectionManagerMessage::Connect(ip, port, rep)) => {
                        rep.reply(
                            connect(
                                ip,
                                port,
                                &mut next_connection_id,
                                &mut endpoint,
                                &mut connections,
                                callbacks.as_ref()
                            ).await
                        );
                    }

                    Some(ConnectionManagerMessage::SendDatagram(id, bytes, rep)) => {
                        rep.reply(
                            send_datagram(id, bytes, &mut connections)
                        );
                    }

                    Some(ConnectionManagerMessage::SendReqRep(id, bytes, rep)) => {
                        rep.reply(
                            send_req_rep(id, bytes, &mut connections).await
                        );
                    }

                    Some(ConnectionManagerMessage::Disconnect(id, rep)) => {
                        disconnect(id, &mut connections, callbacks.as_ref()).await;
                        rep.reply(());
                    }

                    Some(ConnectionManagerMessage::Stop(rep)) => {
                        stop_reply_channel = Some(rep);
                        break 'main_loop;
                    }
                }
            }

            maybe_connecting = incoming.next() => {
                if !process_incoming(&mut next_connection_id, &mut connections, callbacks.as_ref(), maybe_connecting).await {
                    warn!("Local endpoint was stopped, stopping connection manager");
                    break 'main_loop;
                }
            }

            message = get_next_message(&mut connections, callbacks.as_ref()).fuse() => {
                if let Err(f) = process_message(message, callbacks.as_ref(), &mut connections).await {
                    warn!("Failed to handle message due to {}", f);
                }
            }
        };
    }

    // Drop everything, then reply to whoever asked us to stop
    drop(connections);
    drop(incoming);

    endpoint.wait_idle().await;
    drop(endpoint);

    if let Some(x) = stop_reply_channel {
        x.reply(());
    }
}

async fn connect(
    addr: IpAddr,
    port: u16,
    next_connection_id: &mut u32,
    endpoint: &mut Endpoint,
    connections: &mut ConnectionMap,
    callbacks: &dyn ConnectionManagerCallbacks,
) -> Result<ConnectionID> {
    let connection = endpoint
        .connect(SocketAddr::new(addr, port), "mu_peer")
        .context("Failed to connect to peer")?
        .await
        .context("Failed to establish connection")?;

    let (send, recv) = connection
        .connection
        .open_bi()
        .await
        .context("Failed to open bi-directional stream")?;

    // TODO: unify codec builder creation between this and process_incoming
    let mut codec_builder = LengthDelimitedCodec::builder();
    codec_builder.max_frame_length(8 * 1024); // TODO: make this configurable;

    let id = *next_connection_id;
    *next_connection_id += 1;

    info!("Connected to {}:{} with ID {}", addr, port, id);

    connections.insert(
        id,
        (
            connection,
            codec_builder.new_read(recv),
            codec_builder.new_write(send),
        ),
    );

    callbacks.new_connection_available(id).await;

    Ok(id)
}

fn send_datagram(id: ConnectionID, data: Bytes, connections: &mut ConnectionMap) -> Result<()> {
    if let Some(connection) = connections.get_mut(&id) {
        connection.0.connection.send_datagram(data)?;
        return Ok(());
    }

    bail!("Unknown connection ID {}", id);
}

async fn send_req_rep(
    id: ConnectionID,
    data: Bytes,
    connections: &mut ConnectionMap,
) -> Result<Bytes> {
    // TODO: If both peers send a req/rep message at the same time, they'll mistake
    // the request for the other's reply, and the requests will never be processed at all.
    // The easiest solution is to grab two channels per pair of peers instead of one.
    if let Some(connection) = connections.get_mut(&id) {
        connection.2.send(data).await?;
        match connection.1.next().await {
            Some(Ok(bytes)) => return Ok(bytes.freeze()),
            Some(Err(f)) => return Err(f.into()),
            None => bail!("Failed to read response because the connection was closed"),
        }
    }

    bail!("Unknown connection ID {}", id);
}

async fn disconnect(
    id: ConnectionID,
    connections: &mut ConnectionMap,
    callbacks: &dyn ConnectionManagerCallbacks,
) {
    if let Some(connection) = connections.remove(&id) {
        // This does nothing really, but it's good to be explicit
        std::mem::drop(connection);

        callbacks.connection_closed(id).await;
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
                "Failed to accept bi-directional stream from {}, won't connect, error is: {}",
                id, f
            );
            return true;
        }
    };

    let mut codec_builder = LengthDelimitedCodec::builder();
    codec_builder.max_frame_length(8 * 1024); // TODO: make this configurable;

    connections.insert(
        id,
        (
            new_connection,
            codec_builder.new_read(recv),
            codec_builder.new_write(send),
        ),
    );
    callbacks.new_connection_available(id).await;

    true
}

enum IncomingMessage {
    Datagram(ConnectionID, Bytes),
    ReqRep(ConnectionID, Bytes),
}

async fn get_next_message(
    connections: &mut ConnectionMap,
    callbacks: &dyn ConnectionManagerCallbacks,
) -> IncomingMessage {
    if connections.len() == 0 {
        // TODO: This will need be re-worked if we move away from select!, since it
        // essentially creates a future that never completes.
        future::pending::<()>().await;
    }
    // TODO: this discards all the futures every time.
    // leaving the matter of performance aside, is this even a correct thing to do?
    // essentially, the question is: are all the futures cancellation-safe?
    // what happens if a LengthDelimited is in the middle of reading a message and
    // it gets cancelled?
    let mut to_disconnect = vec![];
    let result = loop {
        match futures::future::select_all(
            connections
                .iter_mut()
                .filter(|(k, _)| !to_disconnect.contains(*k))
                .map(|(id, connection)| Box::pin(get_next_message_single(*id, connection))),
        )
        .await
        .0
        {
            Ok(msg) => break msg,
            Err(id) => to_disconnect.push(id),
        }
    };

    for id in to_disconnect {
        disconnect(id, connections, callbacks).await;
    }

    result
}

async fn get_next_message_single(
    id: ConnectionID,
    connection: &mut OpenConnection,
) -> Result<IncomingMessage, ConnectionID> {
    loop {
        tokio::select! {
            bi = connection.1.next() => {
                match bi {
                    Some(Ok(bytes)) => return Ok(IncomingMessage::ReqRep(id, bytes.freeze())),
                    Some(Err(f)) => {
                        info!("Failed to read message from connection {} due to {}, removing connection", id, f);
                        return Err(id);
                    }
                    None => {
                        info!("Failed to read message from connection {}, removing connection", id);
                        return Err(id);
                    }
                }
            }

            datagram = connection.0.datagrams.next() => {
                match datagram {
                    Some(Ok(bytes)) => return Ok(IncomingMessage::Datagram(id, bytes)),
                    Some(Err(f)) => {
                        info!("Failed to read message from connection {} due to {}, removing connection", id, f);
                        return Err(id);
                    }
                    None => {
                        info!("Failed to read message from connection {}, removing connection", id);
                        return Err(id);
                    }
                }
            }
        }
    }
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
            let (_, _, send) = connections.get_mut(&id).unwrap();
            // TODO: handle errors returned by user code
            let result = callbacks.req_rep_received(id, bytes).await;
            // TODO: what happens if we lose the connection before writing a reply?
            send.send(result).await?;
        }
    }

    Ok(())
}

struct SkipServerVerification;

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl rustls::client::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}
