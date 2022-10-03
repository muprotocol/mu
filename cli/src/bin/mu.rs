use anyhow::Result;
use clap::Parser;
use mu_cli::Opts;

fn main() -> Result<()> {
    mu_cli::entry(Opts::parse())
}
