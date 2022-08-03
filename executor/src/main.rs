mod config;
mod connection_manager;

use anyhow::Result;
use env_logger::Env;

use log::info;

use connection_manager::ConnectionID;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::initialize_config(vec![
        ("log_level", "warn"),
        ("connection_manager.listen_ip", "0.0.0.0"),
        ("connection_manager.listen_port", "12012"),
    ])?;

    env_logger::Builder::from_env(
        Env::default().default_filter_or(config.get_string("log_level")?),
    )
    .init();

    let connection_manager = connection_manager::start(
        config
            .get_string("connection_manager.listen_address")?
            .parse()?,
        config
            .get_string("connection_manager.listen_port")?
            .parse()?,
        Box::new(CB {}),
    );

    info!("Initializing Mu...");

    // do something!

    info!("Goodbye!");

    Ok(())
}

struct CB {}

impl connection_manager::ConnectionManagerCallbacks for CB {
    fn new_connection_available<'life0, 'async_trait>(
        &'life0 self,
        id: ConnectionID,
    ) -> core::pin::Pin<
        Box<dyn core::future::Future<Output = ()> + core::marker::Send + 'async_trait>,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }

    fn connection_closed<'life0, 'async_trait>(
        &'life0 self,
        id: ConnectionID,
    ) -> core::pin::Pin<
        Box<dyn core::future::Future<Output = ()> + core::marker::Send + 'async_trait>,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }

    fn datagram_received<'life0, 'async_trait>(
        &'life0 self,
        id: ConnectionID,
        data: bytes::Bytes,
    ) -> core::pin::Pin<
        Box<dyn core::future::Future<Output = ()> + core::marker::Send + 'async_trait>,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }

    fn req_rep_received<'life0, 'async_trait>(
        &'life0 self,
        id: ConnectionID,
        data: bytes::Bytes,
    ) -> core::pin::Pin<
        Box<dyn core::future::Future<Output = bytes::Bytes> + core::marker::Send + 'async_trait>,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        todo!()
    }
}
