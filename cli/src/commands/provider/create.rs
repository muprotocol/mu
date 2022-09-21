//! Create provider subcommand

use anyhow::Result;
use clap::{value_t, ArgMatches};

use crate::mu_marketplace::MarketplaceClient;

/// The options for the `mu provider create` subcommand
#[derive(Debug)]
pub struct Create {
    /// Create a new provider
    name: String,
}

impl Create {
    /// Runs logic for the `mu provider create` subcommand
    pub fn execute(self, solana_client: MarketplaceClient) -> Result<()> {
        solana_client.create_provider(self.name)
    }
}

pub(crate) fn parse(matches: &ArgMatches<'_>) -> Result<Create> {
    let name = value_t!(matches, "name", String)?;
    Ok(Create { name })
}
