// Post-prototype:
// * Handle disconnections (automatic reconnect, message queueing, etc.)

// Future improvements:
// * use polling instead of async and select_all to improve performance
// * separate out time-consuming operations, e.g. accepting connections into their own tasks
// * pool bi-directional streams for each connection?

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use anyhow::{bail, format_err, Context, Result};
use async_trait::async_trait;
use bytes::Bytes;
use dyn_clonable::clonable;
use futures::{future, FutureExt, SinkExt, StreamExt};
use log::*;
use mailbox_processor::{
    plain::{MessageReceiver, PlainMailboxProcessor},
    NotificationChannel, ReplyChannel,
};
use quinn::{
    ClientConfig, Connecting, Endpoint, Incoming, NewConnection, RecvStream, SendStream,
    ServerConfig,
};
use serde::Deserialize;
use tokio_util::codec::{length_delimited, FramedRead, FramedWrite, LengthDelimitedCodec};

use super::ConnectionID;

#[derive(Deserialize)]
pub struct ConnectionManagerConfig {
    pub listen_address: IpAddr,
    pub listen_port: u16,
    #[serde(rename = "max_request_response_size_kb")]
    pub max_request_response_size: usize,
}

#[async_trait]
#[clonable]
pub trait ConnectionManager: Clone + Sync + Send {
    async fn connect(&self, address: IpAddr, port: u16) -> Result<ConnectionID>;
    fn send_datagram(&self, id: ConnectionID, data: Bytes);
    async fn send_req_rep(&self, id: ConnectionID, data: Bytes) -> Result<Bytes>;
    async fn send_reply(&self, id: ConnectionID, req_id: RequestID, data: Bytes) -> Result<()>;
    async fn disconnect(&self, id: ConnectionID) -> Result<()>;
    async fn stop(&self) -> Result<()>;
}

#[derive(Debug)]
enum ConnectionManagerMessage {
    Connect(IpAddr, u16, ReplyChannel<Result<ConnectionID>>),
    SendDatagram(ConnectionID, Bytes),
    SendReqRep(ConnectionID, Bytes, ReplyChannel<Result<Bytes>>),
    SendReply(ConnectionID, RequestID, Bytes, ReplyChannel<()>),
    Disconnect(ConnectionID, ReplyChannel<()>),
    Stop(ReplyChannel<()>),
}

#[derive(Debug)]
pub enum ConnectionManagerNotification {
    NewConnectionAvailable(ConnectionID),
    ConnectionClosed(ConnectionID),
    DatagramReceived(ConnectionID, Bytes),
    /// When receiving this notification, a reply must be provided using [`ConnectionManager.send_reply`].
    ReqRepReceived(ConnectionID, RequestID, Bytes),
}

type NotificationSender = NotificationChannel<ConnectionManagerNotification>;

#[derive(Clone)]
struct ConnectionManagerImpl {
    mailbox: PlainMailboxProcessor<ConnectionManagerMessage>,
}

#[async_trait]
impl ConnectionManager for ConnectionManagerImpl {
    async fn connect(&self, address: IpAddr, port: u16) -> Result<ConnectionID> {
        debug!("Sending connect {}:{}", address, port);
        flatten_and_map_result(
            self.mailbox
                .post_and_reply(|r| ConnectionManagerMessage::Connect(address, port, r))
                .await,
        )
    }

    fn send_datagram(&self, id: ConnectionID, data: Bytes) {
        debug!("Sending datagram {} <- {:?}", id, data);
        self.mailbox
            .post_and_forget(ConnectionManagerMessage::SendDatagram(id, data));
    }

    async fn send_req_rep(&self, id: ConnectionID, data: Bytes) -> Result<Bytes> {
        debug!("Sending req-rep {} <- {:?}", id, data);
        // TODO: special handling when reply channel is dropped due to errors?
        flatten_and_map_result(
            self.mailbox
                .post_and_reply(|r| ConnectionManagerMessage::SendReqRep(id, data, r))
                .await,
        )
    }

    async fn send_reply(&self, id: ConnectionID, req_id: RequestID, data: Bytes) -> Result<()> {
        debug!("Sending reply {}.{} <- {:?}", id, req_id, data);
        self.mailbox
            .post_and_reply(|r| ConnectionManagerMessage::SendReply(id, req_id, data, r))
            .await
            .map_err(Into::into)
    }

    async fn disconnect(&self, id: ConnectionID) -> Result<()> {
        debug!("Sending disconnect {}", id);
        self.mailbox
            .post_and_reply(|r| ConnectionManagerMessage::Disconnect(id, r))
            .await
            .map_err(Into::into)
    }

    async fn stop(&self) -> Result<()> {
        debug!("Sending stop");
        self.mailbox
            .post_and_reply(ConnectionManagerMessage::Stop)
            .await
            .map_err(Into::into)
    }
}

fn flatten_and_map_result<T>(r: Result<Result<T>, mailbox_processor::Error>) -> Result<T> {
    match r {
        Ok(Ok(x)) => Ok(x),
        Ok(Err(f)) => Err(f),
        Err(f) => Err(f.into()),
    }
}

pub fn start(
    config: ConnectionManagerConfig,
    notification_sender: NotificationSender,
) -> Result<Box<dyn ConnectionManager>> {
    if config.listen_address.is_unspecified() {
        bail!("Connection manager listen address cannot be the all-zeroes 'unspecified' address");
    }

    info!(
        "Starting connection manager on {}:{}",
        config.listen_address, config.listen_port
    );

    // TODO: make self-signed certificates optional
    let (private_key, cert_chain) = make_self_signed_certificate();
    let server_config = ServerConfig::with_single_cert(cert_chain, private_key)
        .context("Failed to configure server")?;

    let (mut endpoint, incoming) = Endpoint::server(
        server_config,
        SocketAddr::new(config.listen_address, config.listen_port),
    )
    .context("Failed to start server")?;

    let client_config = make_client_configuration();
    endpoint.set_default_client_config(client_config);

    let mut codec_builder = LengthDelimitedCodec::builder();
    codec_builder.max_frame_length(config.max_request_response_size);

    let mailbox = PlainMailboxProcessor::start(
        move |_mb, r| body(r, notification_sender, endpoint, incoming, codec_builder),
        10000,
    );

    Ok(Box::new(ConnectionManagerImpl { mailbox }))
}

pub type RequestID = u32;

struct ReqRepChannel {
    read: FramedRead<RecvStream, LengthDelimitedCodec>,
    write: FramedWrite<SendStream, LengthDelimitedCodec>,
}

struct OpenConnection {
    new_connection: NewConnection, // I have no idea why quinn calls this a "new" connection
    // This is necessary because we can't add new connections to pending_reads and
    // listen for messages from the connections already inside it at the same time
    // (double mutable borrow), or at least I don't know how to do it properly.
    just_received: Vec<ReqRepChannel>,
    pending_reads: HashMap<RequestID, ReqRepChannel>,
    pending_writes: HashMap<RequestID, ReqRepChannel>,
    next_request_id: RequestID,
}

type ConnectionMap = HashMap<ConnectionID, OpenConnection>;

struct ConnectionManagerState {
    endpoint: Endpoint,
    notification_sender: NotificationSender,
    next_connection_id: u32,
    connections: ConnectionMap,
    req_rep_codec_builder: length_delimited::Builder,
}

async fn body(
    mut message_receiver: MessageReceiver<ConnectionManagerMessage>,
    notification_sender: NotificationSender,
    endpoint: Endpoint,
    mut incoming: Incoming,
    req_rep_codec_builder: length_delimited::Builder,
) {
    let mut state = ConnectionManagerState {
        endpoint,
        notification_sender,
        req_rep_codec_builder,
        connections: HashMap::new(),
        next_connection_id: 0,
    };

    let mut stop_reply_channel = None;

    // TODO: this code is not async enough. For example, if connecting to
    // a peer takes a long time, incoming messages won't be processed until
    // it's done.
    'main_loop: loop {
        // TODO select! requires all futures to be cancellation-safe. Are they?
        debug!("Waiting for activity");
        tokio::select! {
            msg = message_receiver.receive() => {
                debug!("Received control message {:?}", msg);
                match msg {
                    None => {
                        warn!("Mailbox was stopped, stopping connection manager");
                        break 'main_loop;
                    }

                    Some(ConnectionManagerMessage::Connect(ip, port, rep)) => {
                        rep.reply(
                            // TODO await
                            connect(ip, port, &mut state).await
                        );
                    }

                    Some(ConnectionManagerMessage::SendDatagram(id, bytes)) => {
                        if let Err(f) = send_datagram(id, bytes, &mut state) {
                            debug!("Failed to send datagram to {id} due to {f}");
                        }
                    }

                    Some(ConnectionManagerMessage::SendReqRep(id, bytes, rep)) => {
                        // TODO this call only waits until a new channel is opened, which
                        // looks like a relatively instantaneous operation, which is good
                        // enough, but it can be made completely async with the main task
                        // if we move each connection to its mailbox and handle it there.
                        send_req_rep(id, bytes, rep, &mut state).await;
                    }

                    Some(ConnectionManagerMessage::SendReply(id, req_id, bytes, r)) => {
                        // TODO await
                        send_reply(id, req_id, bytes, &mut state).await;
                        r.reply(());
                    }

                    Some(ConnectionManagerMessage::Disconnect(id, rep)) => {
                        // TODO await
                        disconnect(id, &mut state).await;
                        rep.reply(());
                    }

                    Some(ConnectionManagerMessage::Stop(rep)) => {
                        stop_reply_channel = Some(rep);
                        break 'main_loop;
                    }
                }
            }

            maybe_connecting = incoming.next() => {
                debug!("Received incoming connection: {:?}", maybe_connecting);
                // TODO await
                if !process_incoming(maybe_connecting, &mut state).await {
                    warn!("Local endpoint was stopped, stopping connection manager");
                    break 'main_loop;
                }
            }

            message = get_next_message(&mut state).fuse() => {
                debug!("Received incoming message: {:?}", message);
                // TODO await
                if let Err(f) = process_message(message, &mut state).await {
                    warn!("Failed to handle message due to {}", f);
                }
            }
        };
    }

    // Drop everything, then reply to whoever asked us to stop
    let ConnectionManagerState {
        connections,
        endpoint,
        ..
    } = state;
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
    state: &mut ConnectionManagerState,
) -> Result<ConnectionID> {
    debug!("Connecting to {addr}:{port}");
    let new_connection = state
        .endpoint
        .connect(SocketAddr::new(addr, port), "mu_peer")
        .context("Failed to connect to peer")?
        .await
        .context("Failed to establish connection")?;

    let id = get_and_increment(&mut state.next_connection_id);

    info!("Connected to {addr}:{port} with ID {id}");

    state.connections.insert(
        id,
        OpenConnection {
            new_connection,
            just_received: vec![],
            pending_reads: HashMap::new(),
            pending_writes: HashMap::new(),
            next_request_id: 0,
        },
    );

    state
        .notification_sender
        .send(ConnectionManagerNotification::NewConnectionAvailable(id));

    Ok(id)
}

fn send_datagram(id: ConnectionID, data: Bytes, state: &mut ConnectionManagerState) -> Result<()> {
    if let Some(connection) = state.connections.get_mut(&id) {
        debug!("Sending datagram {id} <- {data:?}");
        connection.new_connection.connection.send_datagram(data)?;
        return Ok(());
    }

    bail!("Unknown connection ID {id}");
}

async fn send_req_rep(
    id: ConnectionID,
    data: Bytes,
    reply_channel: ReplyChannel<Result<Bytes>>,
    state: &mut ConnectionManagerState,
) {
    if let Some(connection) = state.connections.get_mut(&id) {
        debug!("Sending req-rep message to {} <- {:?}", id, data);

        debug!("Opening req-rep stream");
        let (send, recv) = match connection
            .new_connection
            .connection
            .open_bi()
            .await
            .context("Failed to open bi-directional stream")
        {
            Ok(x) => x,
            Err(f) => {
                reply_channel.reply(Err(f));
                return;
            }
        };

        let mut write = state.req_rep_codec_builder.new_write(send);
        let mut read = state.req_rep_codec_builder.new_read(recv);

        tokio::spawn(async move {
            let reply = async move {
                debug!("Writing req-rep request");
                write
                    .send(data)
                    .await
                    .context("Failed to write req-rep request")?;

                debug!("Waiting for req-rep reply");
                let reply = read
                    .next()
                    .await
                    .map(|r| r.context("Failed to receive req-rep reply"));
                debug!("Received reply: {:?}", reply);

                match reply {
                    Some(Ok(bytes)) => Ok(bytes.freeze()),
                    Some(Err(f)) => Err(f),
                    None => bail!("Failed to read response because the connection was closed"),
                }
            }
            .await;

            reply_channel.reply(reply);
        });
    } else {
        reply_channel.reply(Err(format_err!("Unknown connection ID {}", id)));
    }
}

async fn send_reply(
    id: ConnectionID,
    req_id: RequestID,
    data: Bytes,
    state: &mut ConnectionManagerState,
) {
    debug!("Sending reply {id}.{req_id} <- {data:?}");
    if let Some(connection) = state.connections.get_mut(&id) {
        if let Some(channel) = connection.pending_writes.get_mut(&req_id) {
            if let Err(f) = channel.write.send(data).await {
                info!("Failed to reply to {} due to: {}", id, f);
            }
            // Remove connection after the response is written in case we get cancelled half-way
            connection.pending_writes.remove(&req_id);
        }
    }
}

async fn disconnect(id: ConnectionID, state: &mut ConnectionManagerState) {
    if let Some(connection) = state.connections.remove(&id) {
        debug!("Disconnecting {id}");
        // This does nothing really, but it's good to be explicit
        std::mem::drop(connection);

        state
            .notification_sender
            .send(ConnectionManagerNotification::ConnectionClosed(id));
    }
}

async fn process_incoming(
    maybe_connecting: Option<Connecting>,
    state: &mut ConnectionManagerState,
) -> bool {
    debug!("Accepting new connection");
    let connecting = match maybe_connecting {
        None => return false,
        Some(x) => x,
    };

    // TODO: accept connections on another task
    let new_connection = match connecting.await {
        Ok(x) => x,
        Err(f) => {
            info!("Failed to accept connection due to: {}", f);
            return true;
        }
    };

    let id = get_and_increment(&mut state.next_connection_id);

    info!(
        "New connection available from {}, assigning id {}",
        new_connection.connection.remote_address(),
        id
    );

    state.connections.insert(
        id,
        OpenConnection {
            new_connection,
            just_received: vec![],
            pending_reads: HashMap::new(),
            pending_writes: HashMap::new(),
            next_request_id: 0,
        },
    );

    state
        .notification_sender
        .send(ConnectionManagerNotification::NewConnectionAvailable(id));

    true
}

#[derive(Debug)]
enum IncomingMessage {
    Datagram(ConnectionID, Bytes),
    ReqRep(ConnectionID, RequestID, Bytes),
}

async fn get_next_message(state: &mut ConnectionManagerState) -> IncomingMessage {
    let num_connections = state.connections.len();
    if num_connections == 0 {
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
        match future::select_all(
            state
                .connections
                .iter_mut()
                .filter(|(k, _)| !to_disconnect.contains(*k))
                .map(|(id, connection)| {
                    Box::pin(get_next_message_single(
                        *id,
                        connection,
                        &state.req_rep_codec_builder,
                    ))
                }),
        )
        .await
        .0
        {
            Ok(Some(msg)) => break Some(msg),
            Ok(None) => continue,
            Err(id) => {
                debug!("Failed to receive messages from {id}, disconnecting");
                to_disconnect.push(id);
                if to_disconnect.len() == num_connections {
                    break None;
                }
            }
        }
    };

    for id in to_disconnect {
        disconnect(id, state).await;
    }

    if result.is_none() {
        future::pending::<()>().await;
    }

    result.unwrap()
}

async fn get_next_message_single(
    id: ConnectionID,
    connection: &mut OpenConnection,
    codec_builder: &length_delimited::Builder,
) -> Result<Option<IncomingMessage>, ConnectionID> {
    for channel in connection.just_received.drain(..) {
        let req_id = get_and_increment(&mut connection.next_request_id);
        connection.pending_reads.insert(req_id, channel);
        debug!("Moved channel from {id} to pending_read and assigned request id P{req_id}");
    }

    tokio::select! {
        // Req-Rep messages are handled in two steps.
        // This is step 1, where we accept a bi-directional stream...
        bi = connection.new_connection.bi_streams.next() => {
            debug!("New incoming bi-directional stream from {id}: {bi:?}");
            match bi {
                Some(Ok((send, recv))) => {
                    let write = codec_builder.new_write(send);
                    let read = codec_builder.new_read(recv);
                    let channel = ReqRepChannel { write, read };
                    connection.just_received.push(channel);
                    debug!("Adding new channel to {id}, now have {}", connection.just_received.len());
                    Ok(None)
                },
                Some(Err(f)) => {
                    info!("Failed to accept bi-directional stream from connection {} due to {}, removing connection", id, f);
                    Err(id)
                }
                None => {
                    info!("No more bi-directional streams from connection {}, removing connection", id);
                    Err(id)
                }
            }
        }

        // ... and this is step two, where we wait for a message to
        // arrive on a newly opened channel
        ((req_id, bytes), _, _) = blocking_select_all(
            connection
                .pending_reads
                .iter_mut()
                .map(|(k, v)| Box::pin(async move { (k, v.read.next().await) })),
        ) => {
            debug!("Received request in {id}.{req_id}: {bytes:?}");
            match bytes {
                Some(Ok(bytes)) => {
                    Ok(Some(IncomingMessage::ReqRep(id, *req_id, bytes.freeze())))
                },
                // TODO: we may be disconnecting too aggressively here
                Some(Err(f)) => {
                    info!("Failed to receive data over bi-directional stream {} of connection {} due to {}, removing connection", req_id, id, f);
                    Err(id)
                }
                None => {
                    info!("No data from bi-directional stream {} of connection {}, removing connection", req_id, id);
                    Err(id)
                }
            }
        }

        datagram = connection.new_connection.datagrams.next() => {
            debug!("Received datagram from {id}: {datagram:?}");
            match datagram {
                Some(Ok(bytes)) => Ok(Some(IncomingMessage::Datagram(id, bytes))),
                Some(Err(f)) => {
                    info!("Failed to read message from connection {} due to {}, removing connection", id, f);
                    Err(id)
                }
                None => {
                    info!("Failed to read message from connection {}, removing connection", id);
                    Err(id)
                }
            }
        }
    }
}

async fn process_message(
    message: IncomingMessage,
    state: &mut ConnectionManagerState,
) -> Result<()> {
    match message {
        IncomingMessage::Datagram(id, bytes) => {
            debug!("Raising notification for datagram: {id} <- {bytes:?}");
            state
                .notification_sender
                .send(ConnectionManagerNotification::DatagramReceived(id, bytes));
        }

        IncomingMessage::ReqRep(id, req_id, bytes) => {
            debug!("Processing req-rep: {id}.{req_id} <- {bytes:?}");
            let connection = match state.connections.get_mut(&id) {
                Some(x) => x,
                None => bail!("Failed to find connection when it was supposed to be there"),
            };

            let channel = match connection.pending_reads.remove(&req_id) {
                Some(x) => x,
                None => bail!(
                    "Failed to find channel inside connection when it was supposed to be there"
                ),
            };

            debug!("Moving channel {id}.{req_id} to pending_writes");
            connection.pending_writes.insert(req_id, channel);

            debug!("Raising notification for req-rep {id}.{req_id} <- {bytes:?}");
            state
                .notification_sender
                .send(ConnectionManagerNotification::ReqRepReceived(
                    id, req_id, bytes,
                ));
        }
    }

    Ok(())
}

// TODO: move the rest of this somewhere

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

fn get_and_increment(x: &mut u32) -> u32 {
    let r = *x;
    *x += 1;
    r
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

// A variation on select_all which blocks indefinitely instead of
// panicking when the iterator is empty. Useful inside select! blocks.
fn blocking_select_all<I>(iter: I) -> BlockingSelectAll<I::Item>
where
    I: IntoIterator,
    I::Item: future::Future + Unpin,
{
    let v = iter.into_iter().collect::<Vec<_>>();
    if v.is_empty() {
        BlockingSelectAll { sa: None }
    } else {
        BlockingSelectAll {
            sa: Some(future::select_all(v)),
        }
    }
}

struct BlockingSelectAll<Fut> {
    sa: Option<future::SelectAll<Fut>>,
}

impl<Fut: future::Future + Unpin> future::Future for BlockingSelectAll<Fut> {
    type Output = <future::SelectAll<Fut> as future::Future>::Output;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match &mut self.sa {
            Some(x) => x.poll_unpin(cx),
            None => std::task::Poll::Pending,
        }
    }
}
