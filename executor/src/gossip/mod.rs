use futures::prelude::*;

use std::collections::{HashMap, HashSet};
use std::error::Error;

use futures::channel::mpsc::{self, UnboundedReceiver, UnboundedSender};
use futures::future;
use serde::{Deserialize, Serialize};
use tokio::{
    net::{TcpListener, TcpStream},
    time::{self, Duration},
};
use tokio_serde::{formats::SymmetricalBincode, SymmetricallyFramed};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use rand::RngCore;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Node {
    pub address: String,
    pub port: u16,
    pub hash: NodeHash,
}

impl Node {
    pub fn new(address: &str, port: u16) -> Node {
        let mut hash = [0; 20];
        rand::thread_rng().fill_bytes(&mut hash);
        Node {
            address: address.to_owned(),
            port,
            hash,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Heartbeat {
    node: Node,
    seq: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
enum MuMessage {
    Heartbeat(Heartbeat),
    Hello(Node),
}

#[derive(Debug)]
enum MuControlMessage {
    Connect(String, u16),
    GetPeers(UnboundedSender<Vec<Peer>>),
}

type MuFramed = SymmetricallyFramed<
    Framed<TcpStream, LengthDelimitedCodec>,
    MuMessage,
    SymmetricalBincode<MuMessage>,
>;

trait MuStream: TryStream<Ok = MuMessage> + Sink<MuMessage> + Unpin {}
impl<T> MuStream for T where T: TryStream<Ok = MuMessage> + Sink<MuMessage> + Unpin {}

type NodeHash = [u8; 20];

struct LinkedPeer<T> {
    link: Option<T>,
    peer: Peer,
}

impl<T> LinkedPeer<T> {
    fn new(node: Node, seen_from: Vec<NodeHash>, link: Option<T>) -> LinkedPeer<T> {
        LinkedPeer {
            link,
            peer: Peer {
                node,
                seen_from: seen_from.into_iter().collect(),
                last_heartbeat: 0,
            },
        }
    }

    fn peer(&self) -> &Peer {
        &self.peer
    }

    fn last_heartbeat(&self) -> u32 {
        self.peer.last_heartbeat
    }

    fn last_heartbeat_mut(&mut self) -> &mut u32 {
        &mut self.peer.last_heartbeat
    }

    fn node(&self) -> &Node {
        &self.peer.node
    }

    fn seen_from(&self) -> &HashSet<NodeHash> {
        &self.peer.seen_from
    }
    fn seen_from_mut(&mut self) -> &mut HashSet<NodeHash> {
        &mut self.peer.seen_from
    }
}

#[derive(Clone, Debug)]
pub struct Peer {
    pub last_heartbeat: u32,
    pub node: Node,
    pub seen_from: HashSet<NodeHash>,
}

#[derive(Clone)]
pub struct GossipConfig {
    pub heartbeat_time: Duration,
}

impl Default for GossipConfig {
    fn default() -> GossipConfig {
        GossipConfig {
            heartbeat_time: Duration::from_millis(1000),
        }
    }
}

pub struct Gossip {
    control_channel: UnboundedSender<MuControlMessage>,
}

struct GossipState<T> {
    peers: HashMap<NodeHash, LinkedPeer<T>>,
    my_node: Node,
    heartbeat_seq: u32,
    config: GossipConfig,
}

impl<T> GossipState<T>
where
    T: MuStream,
{
    fn new(my_node: Node, config: GossipConfig) -> GossipState<T> {
        GossipState {
            config,
            my_node,
            heartbeat_seq: 0,
            peers: HashMap::new(),
        }
    }

    async fn send_heartbeat(&mut self) {
        let heartbeat = MuMessage::Heartbeat(Heartbeat {
            node: self.my_node.clone(),
            seq: self.heartbeat_seq,
        });

        for peer in self.linked_peers() {
            let r = peer.link.as_mut().unwrap().send(heartbeat.clone()).await;
            if r.is_err() {
                todo!("Remove peer");
            }
        }
        self.heartbeat_seq += 1;
    }

    async fn proc_incoming_conn(&mut self, mut peer_link: T) {
        let hello = peer_link.try_next().await;
        if let Ok(Some(MuMessage::Hello(node))) = hello {
            let r = peer_link.send(MuMessage::Hello(self.my_node.clone())).await;
            if r.is_err() {
                return;
            }
            let node_hash = node.hash.clone();
            let peer = LinkedPeer::new(node, vec![node_hash.clone()], Some(peer_link));
            self.peers.insert(node_hash, peer);
        }
    }

    fn linked_peers(&mut self) -> Vec<&mut LinkedPeer<T>> {
        self.peers
            .values_mut()
            .filter(|peer| peer.link.is_some())
            .collect()
    }

    async fn proc_message(&mut self, sender_hash: &NodeHash, message: MuMessage) {
        println!("{:?}", message);
        match &message {
            MuMessage::Heartbeat(h) => {
                let peer = self.peers.get_mut(&h.node.hash);
                if let Some(peer) = peer {
                    if h.seq <= peer.last_heartbeat() {
                        return;
                    }

                    *peer.last_heartbeat_mut() = h.seq;
                    peer.seen_from_mut().insert(sender_hash.to_owned());
                } else {
                    let mut peer =
                        LinkedPeer::new(h.node.clone(), vec![sender_hash.to_owned()], None);
                    *peer.last_heartbeat_mut() = h.seq;
                    self.peers.insert(h.node.hash.clone(), peer);
                }

                for peer in self
                    .linked_peers()
                    .into_iter()
                    .filter(|p| p.node().hash != *sender_hash)
                {
                    let r = peer.link.as_mut().unwrap().send(message.clone()).await;
                    if r.is_err() {
                        todo!("Remove peer");
                    }
                }
            }
            _ => todo!("Implement message"),
        }
    }
    async fn get_next_message(&mut self) -> (NodeHash, MuMessage) {
        // Otherwise select_all will fail
        if self.peers.len() == 0 {
            future::pending::<()>().await;
        }

        loop {
            let selected = future::select_all(self.linked_peers().into_iter().map(|peer| {
                Box::pin(async {
                    (
                        peer.node().hash.clone(),
                        peer.link.as_mut().unwrap().try_next().await,
                    )
                })
            }));
            let ((sender_hash, message), _, _) = selected.await;

            match message {
                Ok(Some(m)) => break (sender_hash, m),
                _ => todo!("Remove peer"),
            }
        }
    }
}

impl GossipState<MuFramed> {
    async fn start(mut self) -> Result<UnboundedSender<MuControlMessage>, Box<dyn Error>> {
        let listener = TcpListener::bind((self.my_node.address.clone(), self.my_node.port)).await?;
        let (control_tx, mut control_rx) = mpsc::unbounded();

        tokio::spawn(async move {
            let mut heartbeat_interval = time::interval(self.config.heartbeat_time);
            loop {
                futures::select! {
                    _ = heartbeat_interval.tick().fuse() => {
                        self.send_heartbeat().await;
                    }
                    conn = listener.accept().fuse() => {
                        match conn {
                            Ok((socket, _addr)) => {
                                println!("recibo connecto");
                                let framed = Framed::new(socket, LengthDelimitedCodec::new());
                                let peer_link = SymmetricallyFramed::new(framed, SymmetricalBincode::default());
                                self.proc_incoming_conn(peer_link).await;
                            }
                            _ => {}
                        }
                    }
                    event = self.get_next_message().fuse() => {
                        let (sender_hash, message) = event;
                        self.proc_message(&sender_hash, message).await;
                    }
                    control = control_rx.next() => {
                        if let Some(control) = control {
                            self.proc_control_message(control).await;
                        }
                    }
                }
            }
        });

        Ok(control_tx)
    }

    async fn proc_control_message(&mut self, control_message: MuControlMessage) {
        println!("{:?}", control_message);
        match control_message {
            MuControlMessage::Connect(addr, port) => self.proc_connect(&addr, port).await.unwrap(),
            MuControlMessage::GetPeers(reply_channel) => {
                self.proc_get_peers(reply_channel).await.unwrap()
            }
        }
    }

    async fn proc_connect(&mut self, addr: &str, port: u16) -> Result<(), Box<dyn Error>> {
        let socket = TcpStream::connect((addr, port)).await?;
        let framed = Framed::new(socket, LengthDelimitedCodec::new());
        let mut peer_link = SymmetricallyFramed::new(framed, SymmetricalBincode::default());

        peer_link
            .send(MuMessage::Hello(self.my_node.clone()))
            .await?;

        let reply = peer_link.try_next().await?;
        if let Some(MuMessage::Hello(node)) = reply {
            let node_hash = node.hash.clone();
            let peer = LinkedPeer::new(node, vec![node_hash.clone()], Some(peer_link));
            self.peers.insert(node_hash, peer);
        }
        Ok(())
    }

    async fn proc_get_peers(
        &mut self,
        mut reply_channel: UnboundedSender<Vec<Peer>>,
    ) -> Result<(), Box<dyn Error>> {
        let peers = self
            .peers
            .values()
            .map(|linked_peer| linked_peer.peer())
            .cloned()
            .collect();
        reply_channel.send(peers).await?;
        Ok(())
    }
}

impl Gossip {
    pub async fn new(my_node: Node, config: GossipConfig) -> Result<Gossip, Box<dyn Error>> {
        let state = GossipState::new(my_node, config);
        let control_channel = state.start().await?;
        Ok(Gossip { control_channel })
    }

    pub async fn get_peers(&mut self) -> Vec<Peer> {
        let (reply_tx, mut reply_rx) = futures::channel::mpsc::unbounded();
        self.control_channel
            .send(MuControlMessage::GetPeers(reply_tx))
            .await
            .unwrap();
        reply_rx.next().await.unwrap()
    }

    pub async fn connect(&mut self, addr: &str, port: u16) {
        self.control_channel
            .send(MuControlMessage::Connect(addr.to_owned(), port))
            .await
            .unwrap();
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::task::{Context, Poll};
    use futures::Sink;
    use pin_project::pin_project;
    use std::collections::VecDeque;
    use std::pin::Pin;

    #[pin_project]
    pub struct SinkStream<Si, St> {
        #[pin]
        sink: Si,

        #[pin]
        stream: St,
    }

    impl<Si, St> SinkStream<Si, St> {
        pub fn new(sink: Si, stream: St) -> SinkStream<Si, St> {
            SinkStream { sink, stream }
        }

        pub fn _stream(&self) -> &St {
            &self.stream
        }

        pub fn sink(&self) -> &Si {
            &self.sink
        }
        pub fn stream_mut(&mut self) -> &mut St {
            &mut self.stream
        }

        pub fn sink_mut(&mut self) -> &mut Si {
            &mut self.sink
        }
    }

    impl<Si, St, StreamItem> Stream for SinkStream<Si, St>
    where
        St: Stream<Item = StreamItem> + Unpin,
    {
        type Item = StreamItem;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.project().stream.poll_next(cx)
        }
    }

    impl<Si, St, SinkItem, Error> Sink<SinkItem> for SinkStream<Si, St>
    where
        Si: Sink<SinkItem, Error = Error> + Unpin,
    {
        type Error = Error;

        fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            self.project().sink.poll_ready(cx)
        }

        fn start_send(self: Pin<&mut Self>, item: SinkItem) -> Result<(), Self::Error> {
            self.project().sink.start_send(item)
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            self.project().sink.poll_flush(cx)
        }

        fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            self.project().sink.poll_close(cx)
        }
    }

    #[pin_project]
    struct TestStream<T> {
        items: VecDeque<T>,
    }

    impl<T> Stream for TestStream<T> {
        type Item = T;
        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let item = self.project().items.pop_front();
            Poll::Ready(item)
        }
    }

    impl<T> TestStream<T> {
        pub fn new() -> TestStream<T> {
            TestStream {
                items: VecDeque::new(),
            }
        }

        pub fn _items<'a>(&'a self) -> std::collections::vec_deque::Iter<'a, T> {
            self.items.iter()
        }

        pub fn add_item(&mut self, item: T) {
            self.items.push_back(item);
        }
    }

    #[pin_project]
    struct TestSink<T> {
        items: VecDeque<T>,
    }

    impl<T> Sink<T> for TestSink<T> {
        type Error = ();

        fn poll_ready(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
            self.project().items.push_back(item);
            Ok(())
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    impl<T> TestSink<T> {
        pub fn new() -> TestSink<T> {
            TestSink {
                items: VecDeque::new(),
            }
        }

        fn items<'a>(&'a self) -> std::collections::vec_deque::Iter<'a, T> {
            self.items.iter()
        }

        fn clear(&mut self) {
            self.items.clear();
        }
    }

    trait PeerLink<T> {
        fn link(&mut self) -> &mut T;
    }

    impl<T> PeerLink<T> for Option<&mut LinkedPeer<T>> {
        fn link(&mut self) -> &mut T {
            self.as_mut().unwrap().link.as_mut().unwrap()
        }
    }

    impl<T> PeerLink<T> for LinkedPeer<T> {
        fn link(&mut self) -> &mut T {
            self.link.as_mut().unwrap()
        }
    }

    fn new_peer(
        address: &str,
    ) -> (
        LinkedPeer<SinkStream<TestSink<MuMessage>, TestStream<Result<MuMessage, ()>>>>,
        NodeHash,
    ) {
        let other_node = Node::new(address, 1234);

        let link = SinkStream::new(TestSink::new(), TestStream::<Result<_, ()>>::new());

        (
            LinkedPeer::new(
                other_node.clone(),
                vec![other_node.hash.clone()],
                Some(link),
            ),
            other_node.hash,
        )
    }

    #[tokio::test]
    async fn test_proc_incoming_conn() {
        let my_node = Node::new("test", 1234);
        let mut state = GossipState::new(my_node.clone(), Default::default());

        let (mut peer, hash) = new_peer("address_other");

        let peer_node = peer.node().clone();
        peer.link()
            .stream_mut()
            .add_item(Ok::<_, ()>(MuMessage::Hello(peer_node)));

        state.proc_incoming_conn(peer.link()).await;

        assert_eq!(state.peers.keys().collect::<Vec<_>>(), vec![&hash]);
        assert_eq!(
            peer.link().sink().items().collect::<Vec<_>>(),
            vec![&MuMessage::Hello(my_node)],
        );
        println!("{:?}", peer.link.as_mut().unwrap().sink().items());
    }

    #[tokio::test]
    async fn test_proc_heartbeat_from_peer() {
        let mut state = GossipState::new(Node::new("test", 1), Default::default());

        let (peer1, hash1) = new_peer("peer1");
        let (peer2, hash2) = new_peer("peer2");
        let (peer3, hash3) = new_peer("peer3");

        let heartbeat = MuMessage::Heartbeat(Heartbeat {
            node: peer1.node().clone(),
            seq: 1,
        });

        state.peers.insert(hash1.clone(), peer1);
        state.peers.insert(hash2.clone(), peer2);
        state.peers.insert(hash3.clone(), peer3);

        state.proc_message(&hash1, heartbeat.clone()).await;

        //It should update the last_heartbeat field
        assert_eq!(state.peers.get(&hash1).unwrap().last_heartbeat(), 1);

        //It should forward the heartbeat to all peers, except the one it received the heartbeat from
        assert_eq!(state.peers.get_mut(&hash1).link().sink().items().count(), 0);
        assert_eq!(
            state
                .peers
                .get_mut(&hash2)
                .link()
                .sink()
                .items()
                .collect::<Vec<_>>(),
            vec![&heartbeat]
        );
        assert_eq!(
            state
                .peers
                .get_mut(&hash3)
                .link()
                .sink()
                .items()
                .collect::<Vec<_>>(),
            vec![&heartbeat]
        );

        // It should not resend the heartbeat if it's seen it before
        state.peers.get_mut(&hash1).link().sink_mut().clear();
        state.peers.get_mut(&hash2).link().sink_mut().clear();
        state.peers.get_mut(&hash3).link().sink_mut().clear();

        state.proc_message(&hash1, heartbeat.clone()).await;

        assert_eq!(state.peers.get_mut(&hash1).link().sink().items().count(), 0,);
        assert_eq!(state.peers.get_mut(&hash2).link().sink().items().count(), 0,);
        assert_eq!(state.peers.get_mut(&hash3).link().sink().items().count(), 0,);
    }

    #[tokio::test]
    async fn test_proc_heartbeat_from_non_peer() {
        let mut state = GossipState::new(Node::new("test", 1), Default::default());
        let (peer1, hash1) = new_peer("peer1");
        let (peer2, hash2) = new_peer("peer2");
        let (peer3, hash3) = new_peer("peer3");

        let heartbeat = MuMessage::Heartbeat(Heartbeat {
            node: peer2.node().clone(),
            seq: 1,
        });

        state.peers.insert(hash1.clone(), peer1);
        state.peers.insert(hash3.clone(), peer3);

        state.proc_message(&hash1, heartbeat.clone()).await;

        // It should update the last_heartbeat field
        let peer2 = state.peers.get(&hash2).unwrap();
        assert_eq!(peer2.last_heartbeat(), 1);
        assert_eq!(peer2.seen_from().iter().collect::<Vec<_>>(), vec![&hash1]);
        assert!(peer2.link.is_none());

        // It should forward the message to its other peers but not the one it received it from
        assert_eq!(
            state
                .peers
                .get_mut(&hash3)
                .link()
                .sink()
                .items()
                .collect::<Vec<_>>(),
            vec![&heartbeat]
        );

        assert_eq!(state.peers.get_mut(&hash1).link().sink().items().count(), 0);

        // It should not resend the heartbeat if it's seen it before
        state.peers.get_mut(&hash1).link().sink_mut().clear();
        state.peers.get_mut(&hash3).link().sink_mut().clear();

        state.proc_message(&hash1, heartbeat.clone()).await;

        assert_eq!(state.peers.get_mut(&hash1).link().sink().items().count(), 0,);
        assert_eq!(state.peers.get_mut(&hash3).link().sink().items().count(), 0,);
    }
}
