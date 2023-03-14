use std::path::PathBuf;

use anyhow::{Context, Result};

use beau_collector::BeauCollector;
use env_logger::Builder;
use log::LevelFilter;
use mu_stack::{StackID, ValidatedStack};
use tokio_util::sync::CancellationToken;

mod database;
mod runtime;
mod storage;

pub type StackWithID = (ValidatedStack, StackID);

pub async fn start_local_node(stack: StackWithID, project_root: PathBuf) -> Result<()> {
    println!("Starting local mu runtime . . .");

    //TODO: make this configurable
    setup_logging();

    let (runtime, gateway, database, storage, gateways, stack_id) =
        runtime::start(stack, project_root).await?;

    let cancellation_token = CancellationToken::new();
    ctrlc::set_handler({
        let cancellation_token = cancellation_token.clone();
        move || {
            println!("Received SIGINT, stopping ...");
            cancellation_token.cancel()
        }
    })
    .context("Failed to initialize Ctrl+C handler")?;

    println!("Done. The following endpoints are deployed:");
    for gateway in gateways {
        for (mut path, endpoints) in gateway.endpoints {
            //TODO: Fix this in better places, not here
            if path.starts_with('/') {
                path = path[1..].to_string();
            }

            for endpoint in endpoints {
                println!(
                    "\t- {} {}/{path} -> {}:{}",
                    endpoint.method,
                    gateway.name,
                    endpoint.route_to.assembly,
                    endpoint.route_to.function,
                );
            }
        }
    }

    println!("\nStack deployed at: http://localhost:12012/{stack_id}/");

    cancellation_token.cancelled().await;
    [
        runtime.stop().await.map_err(Into::into),
        gateway.stop().await,
        storage.stop().await,
        database.stop().await,
    ]
    .into_iter()
    .bcollect::<()>()
}

fn setup_logging() {
    let mut builder = Builder::new();

    builder.filter_level(LevelFilter::Off);

    builder.filter(Some("mu_function"), LevelFilter::Trace);

    builder.init();
}
