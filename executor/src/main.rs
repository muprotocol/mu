use mu::gossip::{Gossip, Node};

#[tokio::main]
async fn main() {
    println!("Hello, world!");

    let n = Node {
        hash: "Tato".to_owned(),
        address: "pito".to_owned(),
        port: 5,
    };
    let mut g = Gossip::new(n);
    g.start("0.0.0.0:8080".to_owned()).await.unwrap();
}
