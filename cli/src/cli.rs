//! The logic for the mu CLI tool.

use std::{
    env,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use crate::{
    arg_parser::parse_args_and_config,
    commands::Provider,
    error::{AnyhowResultExt, PrettyError},
    mu_marketplace::MarketplaceClient,
};
use anchor_client::{solana_sdk::signer::Signer, Cluster};
use anyhow::{Context, Result};

/// Subcommands for mu Command Line Interface
#[allow(clippy::large_enum_variant)]
pub enum Command {
    /// Provider specific operations
    Provider(Provider),
}

/// The arguments for mu Command Line Interface
pub struct Args {
    pub(crate) keypair: Box<dyn Signer>,
    pub(crate) cluster: Cluster,
    pub(crate) command: Command,
}

impl Args {
    fn execute(self) -> Result<()> {
        let solana_client = MarketplaceClient::new(self.cluster, self.keypair)?;
        match self.command {
            Command::Provider(options) => options.execute(solana_client),
        }
    }
}

/// The main function for the mu CLI tool.
pub fn mu_main() {
    // We allow windows to print properly colors
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();

    let args = parse_args_and_config(env::args_os()).print_and_exit_on_error();

    let exit = Arc::new(AtomicBool::default());
    let _exit = exit.clone();
    // Initialize CTRL-C handler
    ctrlc::set_handler(move || {
        _exit.store(true, Ordering::SeqCst);
    })
    .context("Error setting Ctrl-C handler")
    .print_and_exit_on_error();

    PrettyError::report(args.execute());
}
