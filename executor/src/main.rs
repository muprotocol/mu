use mu::gossip::Node;

#[tokio::main]
async fn main() {
    println!("Hello, world!");

    let _n = Node::new("0.0.0.0", 59999);
    //    let mut g = Gossip::new(n, Default::default()).await.unwrap();
}
