use std::{collections::HashMap, net::IpAddr, str::FromStr, sync::RwLock};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use solana_client::{
    client_error::ClientErrorKind, nonblocking::rpc_client::RpcClient, rpc_request::RpcError,
};
use solana_sdk::signature::{Keypair, Signature};
use spl_token::solana_program::pubkey::Pubkey;

use crate::config::AppConfig;

//TODO: Add Email address too.

#[derive(Debug, Deserialize)]
pub struct AirdropRequest {
    pub amount: u64,
    #[serde(deserialize_with = "deserialize_pubkey")]
    pub to: Pubkey,
    #[serde(default)]
    pub confirm_transaction: bool,
}

#[derive(Serialize)]
pub struct AirdropResponse {
    #[serde(serialize_with = "serialize_signature")]
    pub signature: Signature,
}

#[derive(Debug, Serialize)]
pub enum Error {
    Internal(String),
    FailedToCreateTransaction(String),
    FailedToSendTransaction(String),
    TokenAccountNotInitializedYet,
    PerRequestCapExceeded { requested: u64, capacity: u64 },
    PerAddressCapExceeded { requested: u64, capacity: u64 },
}

pub struct State {
    pub config: AppConfig,
    pub authority_keypair: Keypair,

    pub cache: RwLock<Cache>,
    pub solana_client: RpcClient,
}

#[derive(Default, Debug)]
pub struct Cache {
    pub addr_cache: HashMap<IpAddr, u64>,
    pub pubkey_cache: HashMap<Pubkey, u64>,
}

impl State {
    pub fn check_limits(&self, addr: IpAddr, pubkey: Pubkey, amount: u64) -> Result<(), Error> {
        //if addr.is_loopback() {
        //    return Ok(());
        //}

        if let Some(capacity) = self.config.per_request_cap {
            if amount > capacity {
                return Err(Error::PerRequestCapExceeded {
                    requested: amount,
                    capacity,
                });
            }
        }

        let mut cache = self
            .cache
            .write()
            .map_err(|e| Error::Internal(format!("Can not lock cache: {e}")))?;

        let new_addr_total = cache
            .addr_cache
            .entry(addr)
            .and_modify(|total| *total = total.saturating_add(amount))
            .or_insert(amount);

        if let Some(capacity) = self.config.per_address_cap {
            if *new_addr_total > capacity {
                return Err(Error::PerAddressCapExceeded {
                    requested: amount,
                    capacity,
                });
            }
        }

        let new_pubkey_total = cache
            .pubkey_cache
            .entry(pubkey)
            .and_modify(|total| *total = total.saturating_add(amount))
            .or_insert(amount);

        if let Some(capacity) = self.config.per_address_cap {
            if *new_pubkey_total > capacity {
                return Err(Error::PerAddressCapExceeded {
                    requested: amount,
                    capacity,
                });
            }
        }

        Ok(())
    }

    pub fn revert_changes(&self, addr: IpAddr, pubkey: Pubkey, amount: u64) -> Result<(), Error> {
        let mut cache = self
            .cache
            .write()
            .map_err(|e| Error::Internal(format!("Can not lock cache: {e}")))?;

        cache
            .addr_cache
            .entry(addr)
            .and_modify(|total| *total = total.saturating_sub(amount));

        cache
            .pubkey_cache
            .entry(pubkey)
            .and_modify(|total| *total = total.saturating_sub(amount));
        Ok(())
    }
}

fn deserialize_pubkey<'de, D>(deserializer: D) -> Result<Pubkey, D::Error>
where
    D: Deserializer<'de>,
{
    let pubkey = String::deserialize(deserializer)?;

    Pubkey::from_str(&pubkey)
        .map_err(|e| de::Error::custom(format!("invalid input, expect valid solana pubkey: {e:?}")))
}

fn serialize_signature<S>(sig: &Signature, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    sig.to_string().serialize(serializer)
}

pub async fn account_exists(solana_client: &RpcClient, pubkey: &Pubkey) -> Result<bool, Error> {
    match solana_client.get_account(pubkey).await {
        Ok(_) => Ok(true),
        Err(client_error) => match client_error.kind {
            ClientErrorKind::RpcError(RpcError::ForUser(s)) if s.contains("AccountNotFound") => {
                Ok(false)
            }
            _ => Err(Error::Internal(client_error.to_string())),
        },
    }
}
