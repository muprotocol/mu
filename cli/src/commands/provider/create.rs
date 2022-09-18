//! Create provider subcommand

use anyhow::Result;
use clap::ArgMatches;

use crate::solana_client::SolanaClient;

/// The options for the `mu provider create` subcommand
#[derive(Debug)]
pub struct Create {
    /// Create a new provider
    name: String,
}

impl Create {
    /// Runs logic for the `mu provider create` subcommand
    pub fn execute(self, solana_client: SolanaClient) -> Result<()> {
        solana_client.create_provider(self.name)
    }
}

pub(crate) fn parse(_matches: &ArgMatches<'_>) -> Result<Create> {
    todo!()
}
