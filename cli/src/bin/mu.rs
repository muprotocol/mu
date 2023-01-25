use anyhow::Result;
use clap::Parser;

use mu_cli::Args;

#[tokio::main]
async fn main() -> Result<()> {
    mu_cli::execute(Args::parse()).await
}
