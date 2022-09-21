//! Commands related to providers

use anyhow::Result;
use clap::ArgMatches;
use std::process::exit;

mod create;

use crate::mu_marketplace::MarketplaceClient;
use create::*;

/// The options for the `mu provider` subcommand
#[derive(Debug)]
pub enum Provider {
    /// Create a new provider
    Create(Create),
}

impl Provider {
    /// Runs logic for the `mu provider` subcommand
    pub fn execute(self, solana_client: MarketplaceClient) -> Result<()> {
        match self {
            Self::Create(options) => options.execute(solana_client),
        }
    }
}

pub(crate) fn parse(matches: &ArgMatches<'_>) -> Result<Provider> {
    match matches.subcommand() {
        ("create", Some(matches)) => Ok(Provider::Create(create::parse(matches)?)),
        _ => {
            eprintln!("{}", matches.usage());
            exit(1);
        }
    }
}
