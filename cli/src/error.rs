//! Implements `PrettyError` to print pretty errors in the CLI (when they happen)

use anyhow::Chain;
use colored::*;
use std::fmt::{self, Debug, Write};
use thiserror::Error;

/// Mu CLI Errors
#[derive(Error, Debug)]
pub enum MuCliError {
    /// Can not find Solana config file located at '~/.config/solana/cli/config.yml'
    #[error("Could not open Solana config file: '~/.config/solana/cli/config.yml'")]
    ConfigFileNotFound,
}

/// A `PrettyError` for printing `anyhow::Error` nicely.
pub struct PrettyError {
    error: anyhow::Error,
}

/// A macro that prints a warning with nice colors
#[macro_export]
macro_rules! warning {
    ($($arg:tt)*) => ({
        use colored::*;
        eprintln!("{}: {}", "warning".yellow().bold(), format!($($arg)*));
    })
}

impl PrettyError {
    /// Process a `Result` printing any errors and exiting
    /// the process after
    pub fn report<T>(result: Result<T, anyhow::Error>) -> ! {
        std::process::exit(match result {
            Ok(_t) => 0,
            Err(error) => {
                eprintln!("{:?}", PrettyError { error });
                1
            }
        });
    }
}

pub(crate) trait AnyhowResultExt {
    type T;

    fn print_and_exit_on_error(self) -> Self::T;
}

impl<T> AnyhowResultExt for Result<T, anyhow::Error> {
    type T = T;

    fn print_and_exit_on_error(self) -> T {
        match self {
            Ok(t) => t,
            Err(error) => {
                eprintln!("{:?}", PrettyError { error });
                std::process::exit(1);
            }
        }
    }
}

impl Debug for PrettyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let error = &self.error;

        if f.alternate() {
            return Debug::fmt(&error, f);
        }

        write!(f, "{}", format!("{}: {}", "error".red(), error).bold())?;
        // write!(f, "{}", error)?;

        if let Some(cause) = error.source() {
            // write!(f, "\n{}:", "caused by".bold().blue())?;
            let chain = Chain::new(cause);
            let (total_errors, _) = chain.size_hint();
            for (n, error) in chain.enumerate() {
                writeln!(f)?;
                let mut indented = Indented {
                    inner: f,
                    number: Some(n + 1),
                    is_last: n == total_errors - 1,
                    started: false,
                };
                write!(indented, "{}", error)?;
            }
        }
        Ok(())
    }
}

struct Indented<'a, D> {
    inner: &'a mut D,
    number: Option<usize>,
    started: bool,
    is_last: bool,
}

impl<T> Write for Indented<'_, T>
where
    T: Write,
{
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for (i, line) in s.split('\n').enumerate() {
            if !self.started {
                self.started = true;
                match self.number {
                    Some(number) => {
                        if !self.is_last {
                            write!(
                                self.inner,
                                "{} {: >4} ",
                                "│".bold().blue(),
                                format!("{}:", number).dimmed()
                            )?
                        } else {
                            write!(
                                self.inner,
                                "{}{: >2}: ",
                                "╰─▶".bold().blue(),
                                format!("{}", number).bold().blue()
                            )?
                        }
                    }
                    None => self.inner.write_str("    ")?,
                }
            } else if i > 0 {
                self.inner.write_char('\n')?;
                if self.number.is_some() {
                    self.inner.write_str("       ")?;
                } else {
                    self.inner.write_str("    ")?;
                }
            }

            self.inner.write_str(line)?;
        }

        Ok(())
    }
}
