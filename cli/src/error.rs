use anchor_client::solana_client::client_error::ClientErrorKind as SolanaClientErrorKind;
use anchor_client::solana_client::rpc_request::{RpcError, RpcResponseErrorData};
use anchor_client::solana_sdk::instruction::InstructionError;
use anchor_client::solana_sdk::transaction::TransactionError;
use anchor_client::{solana_sdk::signature::Signature, ClientError};

use anyhow::{anyhow, Result};

#[derive(Debug)]
pub enum CliError {
    InstructionError(u8, InstructionError),
    UnhandledError(ClientError),
    UnexpectedError(anyhow::Error),
}

pub trait MarketplaceResultExt {
    type T;
    type E;

    fn parse_error<F>(self, mapper: F) -> Result<Self::T>
    where
        F: Fn(CliError) -> anyhow::Error;
}

impl MarketplaceResultExt for Result<Signature, ClientError> {
    type T = Signature;
    type E = ClientError;

    fn parse_error<F>(self, mapper: F) -> Result<Self::T>
    where
        F: Fn(CliError) -> anyhow::Error,
    {
        self.map_err(|error| match error {
            ClientError::SolanaClientError(ref solana_error) => match &solana_error.kind {
                SolanaClientErrorKind::RpcError(rpc_error) => match rpc_error {
                    RpcError::RpcResponseError { data, .. } => match data {
                        RpcResponseErrorData::SendTransactionPreflightFailure(e) => match &e.err {
                            Some(e) => match e {
                                TransactionError::InstructionError(i, e) => {
                                    CliError::InstructionError(i.clone(), e.clone())
                                }
                                _ => CliError::UnhandledError(error),
                            },
                            None => CliError::UnexpectedError(anyhow!(
                                "failed, no error data available"
                            )),
                        },
                        _ => CliError::UnhandledError(error),
                    },
                    _ => CliError::UnhandledError(error),
                },
                _ => CliError::UnhandledError(error),
            },
            e => CliError::UnhandledError(e),
        })
        .map_err(mapper)
    }
}
