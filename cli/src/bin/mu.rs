use anyhow::Result;
use clap::Parser;

use mu_cli::Args;

fn main() -> Result<()> {
    mu_cli::execute(Args::parse())
}
