//! A dummy Great subcommand

use anyhow::Result;
use clap::Parser;

/// The options for the `mu great` subcommand
#[derive(Debug, Parser)]
pub struct Great {}

impl Great {
    /// Runs logic for the `mu great` subcommand
    pub fn execute(&self) -> Result<()> {
        println!("Hola!");
        Ok(())
    }
}
