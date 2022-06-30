use futures::prelude::*;

use std::collections::{HashMap, HashSet};
use std::error::Error;

use futures::future;
use serde::{Deserialize, Serialize};
use tokio::{
    net::{TcpListener, TcpStream},
    time::{interval, Duration},
};
use tokio_serde::{formats::SymmetricalBincode, SymmetricallyFramed};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[derive(Serialize, Deserialize, Clone)]
pub struct Node {
    pub address: String,
    pub port: u16,
    pub hash: NodeHash,
}

#[derive(Serialize, Deserialize, Clone)]
struct Heartbeat {
    node: Node,
    seq: u32,
}

#[derive(Serialize, Deserialize, Clone)]
enum MuMessage {
    Heartbeat(Heartbeat),
    Hello(Node),
}

type MuFramed = SymmetricallyFramed<
    Framed<TcpStream, LengthDelimitedCodec>,
    MuMessage,
    SymmetricalBincode<MuMessage>,
>;

trait MuStream: TryStream<Ok = MuMessage> + Sink<MuMessage> + Unpin {}
impl<T> MuStream for T where T: TryStream<Ok = MuMessage> + Sink<MuMessage> + Unpin {}

type NodeHash = String;

struct Peer<T> {
    link: Option<T>,
    last_heartbeat: u32,
    node: Node,
    seen_from: HashSet<NodeHash>,
}

pub struct Gossip {
    peers: HashMap<NodeHash, Peer<MuFramed>>,
    my_node: Node,
}

impl Gossip {
    pub fn new(my_node: Node) -> Gossip {
        Gossip {
            peers: HashMap::new(),
            my_node,
        }
    }

    async fn send_heartbeat<T>(peers: &mut HashMap<NodeHash, Peer<T>>, my_node: &Node, seq: u32)
    where
        T: MuStream,
    {
        let heartbeat = MuMessage::Heartbeat(Heartbeat {
            node: my_node.clone(),
            seq,
        });

        for peer in peers.values_mut().filter(|v| v.link.is_some()) {
            let r = peer.link.as_mut().unwrap().send(heartbeat.clone()).await;
            if r.is_err() {
                todo!("Remove peer");
            }
        }
    }

    async fn proc_incoming_conn<T>(
        peers: &mut HashMap<NodeHash, Peer<T>>,
        my_node: &Node,
        mut peer_link: T,
    ) where
        T: MuStream,
    {
        let hello = peer_link.try_next().await;
        if let Ok(Some(MuMessage::Hello(node))) = hello {
            let r = peer_link.send(MuMessage::Hello(my_node.clone())).await;
            if r.is_err() {
                return;
            }
            let node_hash = node.hash.clone();
            let peer = Peer {
                last_heartbeat: 0,
                seen_from: vec![node_hash.clone()].into_iter().collect(),
                node,
                link: Some(peer_link),
            };
            peers.insert(node_hash, peer);
        }
    }

    async fn proc_message<T>(
        peers: &mut HashMap<NodeHash, Peer<T>>,
        sender_hash: String,
        message: MuMessage,
    ) where
        T: MuStream,
    {
        match message {
            MuMessage::Heartbeat(h) => {
                let peer = peers.get_mut(&h.node.hash);
                if let Some(peer) = peer {
                    peer.last_heartbeat = h.seq;
                    peer.seen_from.insert(sender_hash);
                } else {
                    peers.insert(
                        h.node.hash.clone(),
                        Peer {
                            node: h.node,
                            link: None,
                            last_heartbeat: h.seq,
                            seen_from: vec![sender_hash].into_iter().collect(),
                        },
                    );
                }
            }
            _ => todo!("Implement message"),
        }
    }
    async fn get_next_message<T>(peers: &mut HashMap<NodeHash, Peer<T>>) -> (NodeHash, MuMessage)
    where
        T: MuStream,
    {
        // Otherwise select_all will fail
        if peers.len() == 0 {
            future::pending::<()>().await;
        }

        loop {
            let selected =
                future::select_all(peers.values_mut().filter(|v| v.link.is_some()).map(|peer| {
                    Box::pin(async {
                        (
                            peer.node.hash.clone(),
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

    pub async fn start(&mut self, addr: String) -> Result<(), Box<dyn Error>> {
        let listener = TcpListener::bind(addr).await?;

        let mut heartbeat_interval = interval(Duration::from_millis(1000));
        let mut seq = 0;

        loop {
            futures::select! {
                _ = heartbeat_interval.tick().fuse() => {
                    println!("HARTOBITO");
                    Self::send_heartbeat(&mut self.peers, &self.my_node, seq).await;
                    seq += 1;
                }

                conn = listener.accept().fuse() => {
                    match conn {
                        Ok((socket, _addr)) => {
                            let framed = Framed::new(socket, LengthDelimitedCodec::new());
                            let peer_link = SymmetricallyFramed::new(framed, SymmetricalBincode::default());
                            Self::proc_incoming_conn(&mut self.peers, &self.my_node, peer_link).await;
                        }
                        _ => {}
                    }
                }

                event = Self::get_next_message(&mut self.peers).fuse() => {
                    let (sender_hash, message) = event;
                    Self::proc_message(&mut self.peers, sender_hash, message).await;
                }
            }
        }
    }

    pub async fn connect(&mut self, addr: String) -> Result<(), Box<dyn Error>> {
        let socket = TcpStream::connect(addr).await?;
        let framed = Framed::new(socket, LengthDelimitedCodec::new());
        let mut peer_link = SymmetricallyFramed::new(framed, SymmetricalBincode::default());

        peer_link
            .send(MuMessage::Hello(self.my_node.clone()))
            .await?;

        let reply = peer_link.try_next().await?;
        if let Some(MuMessage::Hello(node)) = reply {
            let node_hash = node.hash.clone();
            let peer = Peer {
                last_heartbeat: 0,
                seen_from: vec![node_hash.clone()].into_iter().collect(),
                node,
                link: Some(peer_link),
            };

            self.peers.insert(node_hash, peer);
        }
        Ok(())
    }
}
