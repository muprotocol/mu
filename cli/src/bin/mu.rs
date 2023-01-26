use anyhow::Result;
use clap::Parser;

use mu_cli::Arguments;

#[tokio::main]
async fn main() -> Result<()> {
    mu_cli::execute(Arguments::parse()).await
}
