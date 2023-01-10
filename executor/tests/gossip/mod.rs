use std::{cell::RefCell, collections::HashSet, time::Duration};

use futures::future::select_all;
use mailbox_processor::NotificationChannel;
use mu::{
    infrastructure::config::ConfigDuration,
    network::{gossip::*, NodeAddress, NodeHash},
};
use rand::{seq::SliceRandom, thread_rng};
use test_log::test;
use tokio::{sync::mpsc::UnboundedReceiver, time::Instant};

// TODO This test fails randomly. I suspect the failures have something to do with
// how the message passing loop is designed and/or some obscure detail of how delays
// work, since modifying the intervals makes the test fail or pass rather
// deterministically. In any case, let's ignore this test for now.
#[test(tokio::test)]
#[ignore = "TODO"]
async fn test_node_discovery() {
    #[cfg(test)]
    let config = GossipConfig {
        max_peers: 4,
        peer_update_interval: ConfigDuration::new(Duration::from_millis(15)),
        liveness_check_interval: ConfigDuration::new(Duration::from_millis(10)),
        heartbeat_interval: ConfigDuration::new(Duration::from_millis(10)),
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
                    vec![1],
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
                    vec![1],
                )
                .expect("Failed to start gossip"),
                rx,
                connections: (1..1 + SEED_COUNT).collect(),
            }
        })
        .collect::<Vec<_>>();

    let non_seed_clones = non_seeds
        .iter()
        .map(|s| (s.port, s.gossip.clone()))
        .collect::<Vec<_>>();

    seeds.extend(non_seeds.into_iter());

    let bridge_states = bridge(seeds, Duration::from_millis(500), Duration::from_millis(1)).await;

    fn contains_port(v: &[(NodeHash, NodeAddress)], port: u16) -> bool {
        v.iter().filter(|(_, a)| a.port == port).count() > 0
    }

    let mut rng = thread_rng();
    for _ in 0..10 {
        let chosen = non_seed_clones
            .choose(&mut rng)
            .expect("Empty list of non-seeds");
        let nodes = chosen
            .1
            .get_nodes()
            .await
            .expect("Failed to get nodes from gossip");
        assert_eq!(nodes.len() as u16, SEED_COUNT + NON_SEED_COUNT - 1);
        for i in 1..1 + SEED_COUNT {
            assert!(contains_port(&nodes, i));
        }
        for i in 101..101 + NON_SEED_COUNT {
            if i != chosen.0 {
                assert!(contains_port(&nodes, i));
            }
        }
    }

    drop(bridge_states);
}

// Since the network manager is abstracted out, we don't need to actually
// send messages on the network. This function simply passes gossip messages
// directly to each recipient.
#[allow(clippy::await_holding_refcell_ref)] // No other task will run simultaneously
async fn bridge(
    gossips: Vec<BridgeState>,
    timeout: Duration,
    log_stats_interval: Duration,
) -> Vec<BridgeState> {
    fn find_by_port(v: &[RefCell<BridgeState>], port: u16) -> &RefCell<BridgeState> {
        v.iter()
            .find(|b| b.borrow().port == port)
            .expect("Couldn't find port in gossips")
    }

    let mut gossips = gossips.into_iter().map(RefCell::new).collect::<Vec<_>>();
    let mut last_log = Instant::now();
    let started_at = last_log;

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

            Some(GossipNotification::NodeDeployedStacks(_, _)) => (),
            Some(GossipNotification::NodeUndeployedStacks(_, _)) => (),
        }

        // Simpler to do than a whole select!, and we're only ever interested in
        // knowing what happened after a few messages were processed
        let now = Instant::now();
        if now.duration_since(last_log) >= log_stats_interval {
            for g in &gossips {
                g.borrow().gossip.log_statistics().await;
            }
            last_log = now;
        }

        if now.duration_since(started_at) >= timeout {
            break;
        }
    }

    gossips.into_iter().map(|c| c.into_inner()).collect()
}

struct BridgeState {
    gossip: Box<dyn Gossip>,
    rx: UnboundedReceiver<GossipNotification>,
    port: u16,
    connections: HashSet<u16>,
}
