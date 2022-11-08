//! This file contains source copied over from the solana_clap_utils create.
//! The code had to be copied over and refactored due to a difference in the
//! version of clap used by solana-cli and mu-cli.

// NOTE: To use a ledger device, one must enable "blind sign mode" in the
// application's settings of the device's Solana application.
// This is due to the fact that the application only supports a limited
// set of commands, and it does not know about our instructions.

// TODO: display the transaction hash so users can cross-reference with the
// one displayed on the device

use std::sync::Arc;

use anchor_client::solana_sdk::{
    derivation_path::{DerivationPath, DerivationPathError},
    signature::{read_keypair, read_keypair_file},
    signer::Signer,
};
use anyhow::{anyhow, Result};
use solana_clap_utils::keypair::keypair_from_seed_phrase;
use solana_remote_wallet::{
    locator::Locator as RemoteWalletLocator,
    locator::LocatorError as RemoteWalletLocatorError,
    remote_keypair::generate_remote_keypair,
    remote_wallet::{maybe_wallet_manager, RemoteWalletError, RemoteWalletManager},
};
use thiserror::Error;

pub struct SignerFromPathConfig {
    pub skip_seed_phrase_validation: bool,
    pub confirm_key: bool,
}

pub fn signer_from_path(
    path: &str,
    keypair_name: &str,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
    config: &SignerFromPathConfig,
) -> Result<Box<dyn Signer>> {
    let SignerSource {
        kind,
        derivation_path,
        legacy,
    } = parse_signer_source(path)?;
    match kind {
        SignerSourceKind::Prompt => {
            let skip_validation = config.skip_seed_phrase_validation;
            Ok(
                Box::new(
                    keypair_from_seed_phrase(
                        keypair_name,
                        skip_validation,
                        false,
                        derivation_path,
                        legacy,
                    )
                    .map_err(|f| anyhow!("Failed to generate keypair from seed phrase: {f}"))?
                )
            )
        }
        SignerSourceKind::Filepath(path) => match read_keypair_file(&path) {
            Err(e) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("could not read keypair file \"{}\". Run \"solana-keygen new\" to create a keypair file: {}", path, e),
            )
            .into()),
            Ok(file) => Ok(Box::new(file)),
        },
        SignerSourceKind::Stdin => {
            let mut stdin = std::io::stdin();
            Ok(Box::new(read_keypair(&mut stdin).map_err(|f| anyhow!("{f}"))?))
        }
        SignerSourceKind::Usb(locator) => {
            if wallet_manager.is_none() {
                *wallet_manager = maybe_wallet_manager()?;
            }
            if let Some(wallet_manager) = wallet_manager {
                Ok(Box::new(generate_remote_keypair(
                    locator,
                    derivation_path.unwrap_or_default(),
                    wallet_manager,
                    config.confirm_key,
                    keypair_name,
                )?))
            } else {
                Err(RemoteWalletError::NoDeviceFound.into())
            }
        }
    }
}

pub(crate) struct SignerSource {
    pub kind: SignerSourceKind,
    pub derivation_path: Option<DerivationPath>,
    pub legacy: bool,
}

impl SignerSource {
    fn new(kind: SignerSourceKind) -> Self {
        Self {
            kind,
            derivation_path: None,
            legacy: false,
        }
    }

    fn new_legacy(kind: SignerSourceKind) -> Self {
        Self {
            kind,
            derivation_path: None,
            legacy: true,
        }
    }
}

const SIGNER_SOURCE_PROMPT: &str = "prompt";
const SIGNER_SOURCE_FILEPATH: &str = "file";
const SIGNER_SOURCE_USB: &str = "usb";
const SIGNER_SOURCE_STDIN: &str = "stdin";

pub(crate) enum SignerSourceKind {
    Prompt,
    Filepath(String),
    Usb(RemoteWalletLocator),
    Stdin,
}

impl AsRef<str> for SignerSourceKind {
    fn as_ref(&self) -> &str {
        match self {
            Self::Prompt => SIGNER_SOURCE_PROMPT,
            Self::Filepath(_) => SIGNER_SOURCE_FILEPATH,
            Self::Usb(_) => SIGNER_SOURCE_USB,
            Self::Stdin => SIGNER_SOURCE_STDIN,
        }
    }
}

impl std::fmt::Debug for SignerSourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s: &str = self.as_ref();
        write!(f, "{}", s)
    }
}

#[derive(Debug, Error)]
pub(crate) enum SignerSourceError {
    #[error("unrecognized signer source")]
    UnrecognizedSource,
    #[error(transparent)]
    RemoteWalletLocatorError(#[from] RemoteWalletLocatorError),
    #[error(transparent)]
    DerivationPathError(#[from] DerivationPathError),
    #[error(transparent)]
    IoError(#[from] std::io::Error),
}
fn parse_signer_source<S: AsRef<str>>(source: S) -> Result<SignerSource, SignerSourceError> {
    let source = source.as_ref();
    let source = {
        #[cfg(target_family = "windows")]
        {
            // trim matched single-quotes since cmd.exe won't
            let mut source = source;
            while let Some(trimmed) = source.strip_prefix('\'') {
                source = if let Some(trimmed) = trimmed.strip_suffix('\'') {
                    trimmed
                } else {
                    break;
                }
            }
            source.replace("\\", "/")
        }
        #[cfg(not(target_family = "windows"))]
        {
            source.to_string()
        }
    };
    match uriparse::URIReference::try_from(source.as_str()) {
        Err(_) => Err(SignerSourceError::UnrecognizedSource),
        Ok(uri) => {
            if let Some(scheme) = uri.scheme() {
                let scheme = scheme.as_str().to_ascii_lowercase();
                match scheme.as_str() {
                    SIGNER_SOURCE_PROMPT => Ok(SignerSource {
                        kind: SignerSourceKind::Prompt,
                        derivation_path: DerivationPath::from_uri_any_query(&uri)?,
                        legacy: false,
                    }),
                    SIGNER_SOURCE_FILEPATH => Ok(SignerSource::new(SignerSourceKind::Filepath(
                        uri.path().to_string(),
                    ))),
                    SIGNER_SOURCE_USB => Ok(SignerSource {
                        kind: SignerSourceKind::Usb(RemoteWalletLocator::new_from_uri(&uri)?),
                        derivation_path: DerivationPath::from_uri_key_query(&uri)?,
                        legacy: false,
                    }),
                    SIGNER_SOURCE_STDIN => Ok(SignerSource::new(SignerSourceKind::Stdin)),
                    _ => {
                        #[cfg(target_family = "windows")]
                        // On Windows, an absolute path's drive letter will be parsed as the URI
                        // scheme. Assume a filepath source in case of a single character scheme.
                        if scheme.len() == 1 {
                            return Ok(SignerSource::new(SignerSourceKind::Filepath(source)));
                        }
                        Err(SignerSourceError::UnrecognizedSource)
                    }
                }
            } else {
                match source.as_str() {
                    STDOUT_OUTFILE_TOKEN => Ok(SignerSource::new(SignerSourceKind::Stdin)),
                    ASK_KEYWORD => Ok(SignerSource::new_legacy(SignerSourceKind::Prompt)),
                    _ => std::fs::metadata(source.as_str())
                        .map(|_| SignerSource::new(SignerSourceKind::Filepath(source)))
                        .map_err(|err| err.into()),
                }
            }
        }
    }
}

pub const STDOUT_OUTFILE_TOKEN: &str = "-";
pub const ASK_KEYWORD: &str = "ASK";
