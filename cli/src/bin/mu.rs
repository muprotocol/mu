use anyhow::Result;
use clap::Parser;

use mu_cli::Arguments;

fn main() -> Result<()> {
    mu_cli::execute(Arguments::parse())
}
