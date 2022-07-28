use tokio::time::Duration;

use mu::gossip::{Gossip, GossipConfig, Node};

mod runtime;

#[tokio::test]
async fn test_network() {
    let mut gossip1 = Gossip::new(
        Node::new("127.0.0.1", 59001),
        GossipConfig {
            heartbeat_time: Duration::from_millis(1),
        },
    )
    .await
    .unwrap();

    let node2 = Node::new("127.0.0.1", 59002);
    let mut gossip2 = Gossip::new(
        node2.clone(),
        GossipConfig {
            heartbeat_time: Duration::from_millis(1),
        },
    )
    .await
    .unwrap();

    let node3 = Node::new("127.0.0.1", 59003);
    let mut _gossip3 = Gossip::new(
        node3.clone(),
        GossipConfig {
            heartbeat_time: Duration::from_millis(1),
        },
    )
    .await
    .unwrap();

    gossip1.connect("127.0.0.1", 59002).await;
    gossip2.connect("127.0.0.1", 59003).await;

    tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;

    let peers = gossip1.get_peers().await;
    let seen_node2 = peers.iter().find(|n| n.node.port == 59002).unwrap();
    let seen_node3 = peers.iter().find(|n| n.node.port == 59003).unwrap();

    assert_eq!(
        seen_node3.seen_from.iter().collect::<Vec<_>>(),
        vec![&seen_node2.node.hash]
    );
}
