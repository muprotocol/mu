use mu::gossip::{Gossip, Node};

#[tokio::main]
async fn main() {
    println!("Hello, world!");

    let n = Node::new("0.0.0.0", 59999);
    //    let mut g = Gossip::new(n, Default::default());
    //    g.start().await.unwrap();
}
