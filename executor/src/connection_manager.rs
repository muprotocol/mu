// Post-prototype:
// * Handle disconnections (automatic reconnect, message queueing, etc.)
// * Retry failed connections

// Future improvements:
// * use polling instead of async and select_all to improve performance
// * separate out time-consuming operations, e.g. accepting connections into their own tasks
// * pool bidirectional streams for each connection?

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
use tokio_util::codec::{length_delimited, FramedRead, FramedWrite, LengthDelimitedCodec};

pub type ConnectionID = u32;

#[async_trait]
pub trait ConnectionManagerCallbacks: Clone + Send + Sync + 'static {
    async fn new_connection_available(&self, id: ConnectionID);
    async fn connection_closed(&self, id: ConnectionID);
    async fn datagram_received(&self, id: ConnectionID, data: Bytes);
    async fn req_rep_received(&self, id: ConnectionID, data: Bytes) -> Bytes;
}

pub struct ConnectionManagerConfig {
    pub listen_address: IpAddr,
    pub listen_port: u16,
    pub max_request_response_size: usize,
}

impl Default for ConnectionManagerConfig {
    fn default() -> Self {
        Self {
            listen_address: "0.0.0.0".parse().unwrap(),
            listen_port: 12012,
            max_request_response_size: 8 * 1024,
        }
    }
}

#[async_trait]
pub trait ConnectionManager: Sync + Send {
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
    ReplyAvailable(ConnectionID, RequestID, Bytes),
    Disconnect(ConnectionID, ReplyChannel<()>),
    Stop(ReplyChannel<()>),
}

#[derive(Clone)]
pub struct ConnectionManagerImpl<CB: ConnectionManagerCallbacks> {
    callbacks: Option<CB>,
    mailbox: Option<PlainMailboxProcessor<ConnectionManagerMessage>>,
}

#[async_trait]
impl<CB: ConnectionManagerCallbacks> ConnectionManager for ConnectionManagerImpl<CB> {
    async fn connect(&self, address: IpAddr, port: u16) -> Result<ConnectionID> {
        flatten_and_map_result(
            self.mailbox
                .as_ref()
                .unwrap()
                .post_and_reply(|r| ConnectionManagerMessage::Connect(address, port, r))
                .await,
        )
    }

    async fn send_datagram(&self, id: ConnectionID, data: Bytes) -> Result<()> {
        flatten_and_map_result(
            self.mailbox
                .as_ref()
                .unwrap()
                .post_and_reply(|r| ConnectionManagerMessage::SendDatagram(id, data, r))
                .await,
        )
    }

    async fn send_req_rep(&self, id: ConnectionID, data: Bytes) -> Result<Bytes> {
        // TODO: special handling when reply channel is dropped due to errors?
        flatten_and_map_result(
            self.mailbox
                .as_ref()
                .unwrap()
                .post_and_reply(|r| ConnectionManagerMessage::SendReqRep(id, data, r))
                .await,
        )
    }

    async fn disconnect(&self, id: ConnectionID) -> Result<()> {
        self.mailbox
            .as_ref()
            .unwrap()
            .post_and_reply(|r| ConnectionManagerMessage::Disconnect(id, r))
            .await
            .map_err(Into::into)
    }

    async fn stop(&self) -> Result<()> {
        self.mailbox
            .as_ref()
            .unwrap()
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

pub fn new<CB: ConnectionManagerCallbacks>() -> ConnectionManagerImpl<CB> {
    ConnectionManagerImpl {
        callbacks: None,
        mailbox: None,
    }
}

impl<CB: ConnectionManagerCallbacks> ConnectionManagerImpl<CB> {
    pub fn set_callbacks(&mut self, cb: CB) {
        self.callbacks = Some(cb);
    }

    pub async fn start(&mut self, config: ConnectionManagerConfig) -> Result<()> {
        let callbacks = self.callbacks.as_ref().unwrap().clone();

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
            move |mb, r| body(mb, r, callbacks, endpoint, incoming, codec_builder),
            10000,
        );

        self.mailbox = Some(mailbox);

        Ok(())
    }
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

type RequestID = u32;

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

async fn body<CB: ConnectionManagerCallbacks>(
    mailbox: PlainMailboxProcessor<ConnectionManagerMessage>,
    mut message_receiver: MessageReceiver<ConnectionManagerMessage>,
    callbacks: CB,
    mut endpoint: Endpoint,
    mut incoming: Incoming,
    req_rep_codec_builder: length_delimited::Builder,
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
                                &callbacks
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
                            send_req_rep(id, bytes, &mut connections, &req_rep_codec_builder).await
                        );
                    }

                    Some(ConnectionManagerMessage::ReplyAvailable(id, req_id, bytes)) => {
                        send_reply(id, req_id, bytes, &mut connections).await;
                    }

                    Some(ConnectionManagerMessage::Disconnect(id, rep)) => {
                        disconnect(id, &mut connections, &callbacks).await;
                        rep.reply(());
                    }

                    Some(ConnectionManagerMessage::Stop(rep)) => {
                        stop_reply_channel = Some(rep);
                        break 'main_loop;
                    }
                }
            }

            maybe_connecting = incoming.next() => {
                if !process_incoming(&mut next_connection_id, &mut connections, &callbacks, maybe_connecting).await {
                    warn!("Local endpoint was stopped, stopping connection manager");
                    break 'main_loop;
                }
            }

            message = get_next_message(&mut connections, &callbacks, &req_rep_codec_builder).fuse() => {
                if let Err(f) = process_message(message, &callbacks, &mailbox, &mut connections).await {
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

async fn connect<CB: ConnectionManagerCallbacks>(
    addr: IpAddr,
    port: u16,
    next_connection_id: &mut u32,
    endpoint: &mut Endpoint,
    connections: &mut ConnectionMap,
    callbacks: &CB,
) -> Result<ConnectionID> {
    let new_connection = endpoint
        .connect(SocketAddr::new(addr, port), "mu_peer")
        .context("Failed to connect to peer")?
        .await
        .context("Failed to establish connection")?;

    let id = *next_connection_id;
    *next_connection_id += 1;

    info!("Connected to {}:{} with ID {}", addr, port, id);

    connections.insert(
        id,
        OpenConnection {
            new_connection,
            just_received: vec![],
            pending_reads: HashMap::new(),
            pending_writes: HashMap::new(),
            next_request_id: 0,
        },
    );

    let cb = callbacks.clone();
    tokio::spawn(async move { cb.new_connection_available(id).await });

    Ok(id)
}

fn send_datagram(id: ConnectionID, data: Bytes, connections: &mut ConnectionMap) -> Result<()> {
    if let Some(connection) = connections.get_mut(&id) {
        connection.new_connection.connection.send_datagram(data)?;
        return Ok(());
    }

    bail!("Unknown connection ID {}", id);
}

async fn send_req_rep(
    id: ConnectionID,
    data: Bytes,
    connections: &mut ConnectionMap,
    codec_builder: &length_delimited::Builder,
) -> Result<Bytes> {
    if let Some(connection) = connections.get_mut(&id) {
        // TODO: handle waiting for a reply outside the main Task
        let (send, recv) = connection
            .new_connection
            .connection
            .open_bi()
            .await
            .context("Failed to open bi-directional stream")?;

        let mut write = codec_builder.new_write(send);
        let mut read = codec_builder.new_read(recv);

        write.send(data).await?;

        match read.next().await {
            Some(Ok(bytes)) => return Ok(bytes.freeze()),
            Some(Err(f)) => return Err(f.into()),
            None => bail!("Failed to read response because the connection was closed"),
        }
    }

    bail!("Unknown connection ID {}", id);
}

async fn send_reply(
    id: ConnectionID,
    req_id: RequestID,
    data: Bytes,
    connections: &mut ConnectionMap,
) {
    if let Some(connection) = connections.get_mut(&id) {
        if let Some(channel) = connection.pending_writes.get_mut(&req_id) {
            if let Err(f) = channel.write.send(data).await {
                info!("Failed to reply to {} due to: {}", id, f);
            }
            // Remove connection after the response is written in case we get cancelled half-way
            connection.pending_writes.remove(&req_id);
        }
    }
}

async fn disconnect<CB: ConnectionManagerCallbacks>(
    id: ConnectionID,
    connections: &mut ConnectionMap,
    callbacks: &CB,
) {
    if let Some(connection) = connections.remove(&id) {
        // This does nothing really, but it's good to be explicit
        std::mem::drop(connection);

        let cb = callbacks.clone();
        tokio::spawn(async move { cb.connection_closed(id).await });
    }
}

async fn process_incoming<CB: ConnectionManagerCallbacks>(
    next_connection_id: &mut u32,
    connections: &mut ConnectionMap,
    callbacks: &CB,
    maybe_connecting: Option<Connecting>,
) -> bool {
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

    let id = *next_connection_id;
    *next_connection_id += 1;

    info!(
        "New connection available from {}, assigning id {}",
        new_connection.connection.remote_address(),
        id
    );

    connections.insert(
        id,
        OpenConnection {
            new_connection,
            just_received: vec![],
            pending_reads: HashMap::new(),
            pending_writes: HashMap::new(),
            next_request_id: 0,
        },
    );

    let cb = callbacks.clone();
    tokio::spawn(async move { cb.new_connection_available(id).await });

    true
}

enum IncomingMessage {
    Datagram(ConnectionID, Bytes),
    ReqRep(ConnectionID, RequestID, Bytes),
}

async fn get_next_message<CB: ConnectionManagerCallbacks>(
    connections: &mut ConnectionMap,
    callbacks: &CB,
    codec_builder: &length_delimited::Builder,
) -> IncomingMessage {
    let num_connections = connections.len();
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
            connections
                .iter_mut()
                .filter(|(k, _)| !to_disconnect.contains(*k))
                .map(|(id, connection)| {
                    Box::pin(get_next_message_single(*id, connection, codec_builder))
                }),
        )
        .await
        .0
        {
            Ok(Some(msg)) => break Some(msg),
            Ok(None) => continue,
            Err(id) => {
                to_disconnect.push(id);
                if to_disconnect.len() == num_connections {
                    break None;
                }
            }
        }
    };

    for id in to_disconnect {
        disconnect(id, connections, callbacks).await;
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
    }

    tokio::select! {
        // Req-Rep messages are handled in two steps.
        // This is step 1, where we accept a bidirectional stream...
        bi = connection.new_connection.bi_streams.next() => {
            match bi {
                Some(Ok((send, recv))) => {
                    let write = codec_builder.new_write(send);
                    let read = codec_builder.new_read(recv);
                    let channel = ReqRepChannel { write, read };
                    connection.just_received.push(channel);
                    return Ok(None);
                },
                Some(Err(f)) => {
                    info!("Failed to accept bi-directional stream from connection {} due to {}, removing connection", id, f);
                    return Err(id);
                }
                None => {
                    info!("No more bi-directional streams from connection {}, removing connection", id);
                    return Err(id);
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
            match bytes {
                Some(Ok(bytes)) => {
                    return Ok(Some(IncomingMessage::ReqRep(id, *req_id, bytes.freeze())));
                },
                // TODO: we may be disconnecting too aggressively here
                Some(Err(f)) => {
                    info!("Failed to receive data over bi-directional stream {} of connection {} due to {}, removing connection", req_id, id, f);
                    return Err(id);
                }
                None => {
                    info!("No data from bi-directional stream {} of connection {}, removing connection", req_id, id);
                    return Err(id);
                }
            }
        }

        datagram = connection.new_connection.datagrams.next() => {
            match datagram {
                Some(Ok(bytes)) => return Ok(Some(IncomingMessage::Datagram(id, bytes))),
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

async fn process_message<CB: ConnectionManagerCallbacks>(
    message: IncomingMessage,
    callbacks: &CB,
    mailbox: &PlainMailboxProcessor<ConnectionManagerMessage>,
    connections: &mut ConnectionMap,
) -> Result<()> {
    match message {
        IncomingMessage::Datagram(id, bytes) => {
            let cb = callbacks.clone();
            tokio::spawn(async move { cb.datagram_received(id, bytes.clone()).await });
        }

        IncomingMessage::ReqRep(id, req_id, bytes) => {
            let connection = match connections.get_mut(&id) {
                Some(x) => x,
                None => bail!("Failed to find connection when it was supposed to be there"),
            };

            let channel = match connection.pending_reads.remove(&req_id) {
                Some(x) => x,
                None => bail!(
                    "Failed to find channel inside connection when it was supposed to be there"
                ),
            };

            connection.pending_writes.insert(req_id, channel);

            let cb = callbacks.clone();
            let mb = mailbox.clone();

            // TODO: handle errors returned by user code
            tokio::spawn(async move {
                let result = cb.req_rep_received(id, bytes).await;
                mb.post_and_forget(ConnectionManagerMessage::ReplyAvailable(id, req_id, result));
            });
        }
    }

    Ok(())
}

// TODO: move the rest of this somewhere

// TODO: use this everywhere
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
