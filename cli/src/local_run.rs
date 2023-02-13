use std::path::Path;

use anyhow::{Context, Result};

use beau_collector::BeauCollector;
use mu_stack::{Stack, StackID};
use tokio::select;
use tokio_util::sync::CancellationToken;

mod database;
mod runtime;

pub type StackWithID = (Stack, StackID);

pub async fn start_local_node(stack: StackWithID) -> Result<()> {
    //TODO: make this configurable
    env_logger::init();

    let (runtime, gateway, database, gateways, stack_id) = runtime::start(stack).await?;

    let cancellation_token = CancellationToken::new();
    ctrlc::set_handler({
        let cancellation_token = cancellation_token.clone();
        move || {
            println!("Received SIGINT, stopping ...");
            cancellation_token.cancel()
        }
    })
    .context("Failed to initialize Ctrl+C handler")?;

    println!("Following endpoints are deployed:");
    for gateway in gateways {
        for (path, endpoints) in gateway.endpoints {
            for endpoint in endpoints {
                println!(
                    "- {}:{} : {} {}/{path}",
                    endpoint.route_to.assembly,
                    endpoint.route_to.function,
                    endpoint.method,
                    gateway.name
                );
            }
        }
    }

    println!("\nStack deployed at: http://localhost:12012/{stack_id}/");

    tokio::spawn({
        async move {
            loop {
                select! {
                    () = cancellation_token.cancelled() => {
                        [
                            runtime.stop().await.map_err(Into::into),
                            gateway.stop().await,
                            database.stop().await,
                            clean_runtime_cache_dir()
                        ].into_iter().bcollect::<()>().unwrap();
                        break
                    }
                }
            }
        }
    })
    .await?;
    Ok(())
}

pub fn clean_runtime_cache_dir() -> Result<()> {
    std::fs::remove_dir_all(Path::new(runtime::CACHE_PATH)).map_err(Into::into)
}
