//! The logic for the mu CLI tool.

use crate::{commands::Great, error::PrettyError};
use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[clap(name = "mu", about = "Mu CLI tool.", version, author)]
/// The options for mu Command Line Interface
enum MuCLIOptions {
    ///// Deploy a stack to mu.
    //#[clap(name = "deploy")]
    //Deploy(Deploy),
    /// Great subcommand
    #[clap(name = "great")]
    Great(Great),
}

impl MuCLIOptions {
    fn execute(&self) -> Result<()> {
        match self {
            Self::Great(options) => options.execute(),
        }
    }
}

/// The main function for the mu CLI tool.
pub fn mu_main() {
    // We allow windows to print properly colors
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();

    // We try to run mu with the normal arguments.
    // Eg. `mu <SUBCOMMAND>`
    let args = std::env::args().collect::<Vec<_>>();
    let command = args.get(1);
    let options = match command.unwrap_or(&"".to_string()).as_ref() {
        "great" => MuCLIOptions::parse(),
        _ => panic!("invalid sub command"), // TODO: we can run a default subcommand instead
    };

    PrettyError::report(options.execute());
}
