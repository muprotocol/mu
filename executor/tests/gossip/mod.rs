use std::{cell::RefCell, collections::HashSet, time::Duration};

use futures::future::select_all;
use mailbox_processor::NotificationChannel;
use mu::network::gossip::*;
use rand::{seq::SliceRandom, thread_rng};
use test_log::test;
use tokio::{sync::mpsc::UnboundedReceiver, time};

#[test(tokio::test)]
async fn test_node_discovery() {
    #[cfg(test)]
    let config = GossipConfig {
        max_peers: 4,
        peer_update_interval: Duration::from_millis(15),
        liveness_check_interval: Duration::from_millis(5),
        heartbeat_interval: Duration::from_millis(5),
        assume_dead_after_missed_heartbeats: 10,
    };

    const SEED_COUNT: u16 = 2;
    const NON_SEED_COUNT: u16 = 2;

    let mut seeds = (1..1 + SEED_COUNT)
        .map(|i| {
            let (channel, rx) = NotificationChannel::new();
            BridgeState {
                port: i,
                gossip: start(
                    NodeAddress {
                        address: "127.0.0.1".parse().unwrap(),
                        port: i,
                        generation: 1,
                    },
                    config.clone(),
                    vec![],
                    channel,
                )
                .expect("Failed to start gossip"),
                rx,
                connections: HashSet::new(),
            }
        })
        .collect::<Vec<_>>();

    let non_seeds = (101..101 + NON_SEED_COUNT)
        .map(|i| {
            let (channel, rx) = NotificationChannel::new();
            BridgeState {
                port: i,
                gossip: start(
                    NodeAddress {
                        address: "127.0.0.1".parse().unwrap(),
                        port: i,
                        generation: 1,
                    },
                    config.clone(),
                    (1..1 + SEED_COUNT)
                        .map(|i| {
                            (
                                NodeAddress {
                                    address: "127.0.0.1".parse().unwrap(),
                                    port: i,
                                    generation: 0,
                                },
                                i as u32,
                            )
                        })
                        .collect(),
                    channel,
                )
                .expect("Failed to start gossip"),
                rx,
                connections: (1..1 + SEED_COUNT).collect(),
            }
        })
        .collect::<Vec<_>>();

    let non_seed_clones = non_seeds
        .iter()
        .map(|s| s.gossip.clone())
        .collect::<Vec<_>>();

    seeds.extend(non_seeds.into_iter());

    bridge(seeds, Duration::from_millis(100)).await;

    fn contains_port(v: &Vec<(u128, NodeAddress)>, port: u16) -> bool {
        v.iter().filter(|(_, a)| a.port == port).count() > 0
    }

    let mut rng = thread_rng();
    for _ in 0..10 {
        let chosen = non_seed_clones
            .choose(&mut rng)
            .expect("Empty list of non-seeds");
        let nodes = chosen
            .get_nodes()
            .await
            .expect("Failed to get nodes from gossip");
        assert_eq!(nodes.len() as u16, SEED_COUNT + NON_SEED_COUNT);
        for i in 1..1 + SEED_COUNT {
            assert!(contains_port(&nodes, i));
        }
        for i in 101..101 + NON_SEED_COUNT {
            assert!(contains_port(&nodes, i));
        }
    }
}

// Since the network manager is abstracted out, we don't need to actually
// send messages on the network. This function simply passes gossip messages
// directly to each recipient.
async fn bridge(gossips: Vec<BridgeState>, timeout: Duration) {
    fn find_by_port(v: &Vec<RefCell<BridgeState>>, port: u16) -> &RefCell<BridgeState> {
        v.iter()
            .filter(|b| b.borrow().port == port)
            .next()
            .expect("Couldn't find port in gossips")
    }

    let do_bridge = async move {
        let mut gossips = gossips.into_iter().map(RefCell::new).collect::<Vec<_>>();
        'main_loop: loop {
            let ((msg, port), _, _) = select_all(gossips.iter_mut().map(|g| {
                Box::pin(async {
                    let notification = g.get_mut().rx.recv().await;
                    (notification, g.get_mut().port)
                })
            }))
            .await;

            let gossip = find_by_port(&gossips, port);

            println!("{msg:?} from {port}");

            match msg {
                None => panic!("Gossip {port} was terminated prematurely"),

                Some(GossipNotification::NodeDied(_, _)) => (),
                Some(GossipNotification::NodeDiscovered(_)) => (),

                Some(GossipNotification::Connect(req_id, _, target_port)) => {
                    // We use the ports themselves as connection ID.
                    // TODO how to test new connection ID?
                    let target = find_by_port(&gossips, target_port);
                    target.borrow_mut().connections.insert(port);
                    gossip.borrow_mut().connections.insert(target_port);
                    gossip
                        .borrow_mut()
                        .gossip
                        .connection_available(req_id, target_port as u32);
                }

                Some(GossipNotification::SendMessage(connection_id, bytes)) => {
                    let target_port = connection_id as u16;
                    if !gossip.borrow_mut().connections.contains(&target_port) {
                        continue 'main_loop;
                    }
                    let target = find_by_port(&gossips, target_port);
                    target
                        .borrow_mut()
                        .gossip
                        .receive_message(port as u32, bytes);
                }

                Some(GossipNotification::Disconnect(connection_id)) => {
                    let target_port = connection_id as u16;
                    let target = find_by_port(&gossips, target_port);
                    gossip.borrow_mut().connections.remove(&target_port);
                    target.borrow_mut().connections.remove(&port);
                }
            }
        }
    };

    match time::timeout(timeout, do_bridge).await {
        Ok(()) => panic!("Bridge function exited early"),
        Err(_) => (),
    }
}

struct BridgeState {
    gossip: Box<dyn Gossip>,
    rx: UnboundedReceiver<GossipNotification>,
    port: u16,
    connections: HashSet<u16>,
}

// #[tokio::test]
// async fn test_network() {
//     let mut gossip1 = GossipImpl::new(
//         NodeAddress::new("127.0.0.1", 59001),
//         GossipConfig {
//             heartbeat_time: Duration::from_millis(1),
//         },
//     )
//     .await
//     .unwrap();

//     let node2 = NodeAddress::new("127.0.0.1", 59002);
//     let mut gossip2 = GossipImpl::new(
//         node2.clone(),
//         GossipConfig {
//             heartbeat_time: Duration::from_millis(1),
//         },
//     )
//     .await
//     .unwrap();

//     let node3 = NodeAddress::new("127.0.0.1", 59003);
//     let mut _gossip3 = GossipImpl::new(
//         node3.clone(),
//         GossipConfig {
//             heartbeat_time: Duration::from_millis(1),
//         },
//     )
//     .await
//     .unwrap();

//     gossip1.connect("127.0.0.1", 59002).await;
//     gossip2.connect("127.0.0.1", 59003).await;

//     tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;

//     let peers = gossip1.get_peers().await;
//     let seen_node2 = peers.iter().find(|n| n.node.port == 59002).unwrap();
//     let seen_node3 = peers.iter().find(|n| n.node.port == 59003).unwrap();

//     assert_eq!(
//         seen_node3.seen_from.iter().collect::<Vec<_>>(),
//         vec![&seen_node2.node.hash]
//     );
// }
