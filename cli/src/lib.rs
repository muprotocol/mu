//! The Mu binary lib

#![deny(
    missing_docs,
    dead_code,
    nonstandard_style,
    unused_mut,
    unused_variables,
    unused_unsafe,
    unreachable_patterns,
    unstable_features
)]

//#[macro_use]
//extern crate anyhow;

pub mod cli;
pub mod commands;
pub mod common;
pub mod error;

/// Version number for this crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
