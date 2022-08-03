pub mod config;

use std::net::{Ipv4Addr, SocketAddr};

use anyhow::{Context, Result};
use env_logger::Env;
use mu::gossip::{Gossip, Node};

use log::{info, LevelFilter};

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::initialize_config()?;

    env_logger::Builder::from_env(
        Env::default().default_filter_or(config.get_string("log-level")?),
    )
    .init();

    info!("Initializing Mu...");

    // do something!

    info!("Goodbye!");

    Ok(())
}
