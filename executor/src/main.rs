use std::net::{Ipv4Addr, SocketAddr};

use env_logger::Env;
use mu::gossip::{Gossip, Node};

use log::{info, LevelFilter};

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    info!("Hello, logging!");

    // println!("Hello, world!");

    // let n = Node::new("0.0.0.0", 59999);
    //    let mut g = Gossip::new(n, Default::default());
    //    g.start().await.unwrap();
}
