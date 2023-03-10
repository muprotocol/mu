use log::error;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use solana_client::{
    client_error::ClientErrorKind, nonblocking::rpc_client::RpcClient, rpc_request::RpcError,
};
use std::{
    collections::{hash_map::Entry, HashMap},
    net::IpAddr,
    str::FromStr,
    sync::Mutex,
};

use solana_sdk::{
    hash::Hash,
    signature::{Keypair, Signature},
    signer::Signer,
    transaction::Transaction,
};
use spl_token::solana_program::pubkey::Pubkey;

use crate::{config::AppConfig, database::Database, marketplace::get_token_decimals};

#[derive(Debug, Deserialize)]
pub struct AirdropRequest {
    #[serde(deserialize_with = "deserialize_email")]
    pub email: String,
    pub amount: f64,
    #[serde(deserialize_with = "deserialize_pubkey")]
    pub to: Pubkey,
}

#[derive(Serialize)]
pub struct AirdropResponse {
    #[serde(serialize_with = "serialize_signature")]
    pub signature: Signature,
}

#[derive(Debug, Serialize)]
pub enum Error {
    FailedToProcessTransaction,
    PerRequestCapExceeded { requested: f64, capacity: f64 },
    PerAddressCapExceeded { requested: f64, capacity: f64 },
    PerAccountCapExceeded { requested: f64, capacity: f64 },
}

pub struct State {
    pub config: AppConfig,
    pub authority_keypair: Keypair,

    pub cache: Mutex<Cache>,
    pub database: Database,
    pub solana_client: RpcClient,
    pub token_decimals: u8,
}

#[derive(Default, Debug)]
pub struct Cache {
    pub addr_cache: HashMap<IpAddr, f64>,
    pub pubkey_cache: HashMap<Pubkey, f64>,
}

impl State {
    pub async fn init(config: AppConfig) -> Result<Self, Error> {
        let authority_keypair = config.authority_keypair().expect("read authority keypair");
        let solana_client = RpcClient::new(config.rpc_address.clone());

        let token_decimals = get_token_decimals(&solana_client, &config.marketplace_id)
            .await
            .map_err(|e| {
                error!("Can not get token decimals: {e:?}");
                Error::FailedToProcessTransaction
            })?;

        Ok(Self {
            config,
            authority_keypair,
            cache: Default::default(),
            database: Database::open().expect("open database"),
            token_decimals,
            solana_client,
        })
    }

    pub fn check_limits(&self, addr: IpAddr, pubkey: Pubkey, amount: f64) -> Result<(), Error> {
        if let Some(capacity) = self.config.per_request_cap {
            if amount > capacity {
                return Err(Error::PerRequestCapExceeded {
                    requested: amount,
                    capacity,
                });
            }
        }

        let mut cache = self.cache.lock().map_err(|e| {
            error!("Can not lock cache: {e:?}");
            Error::FailedToProcessTransaction
        })?;

        if let Some(capacity) = self.config.per_address_cap {
            match cache.addr_cache.entry(addr) {
                Entry::Vacant(a) if amount <= capacity => {
                    a.insert(amount);
                }
                Entry::Occupied(mut a) if a.get() + amount <= capacity => {
                    *a.get_mut() = a.get() + amount;
                }
                _ => {
                    return Err(Error::PerAddressCapExceeded {
                        requested: amount,
                        capacity,
                    });
                }
            };
        }

        if let Some(capacity) = self.config.per_account_cap {
            match cache.pubkey_cache.entry(pubkey) {
                Entry::Vacant(a) if amount <= capacity => {
                    a.insert(amount);
                }
                Entry::Occupied(mut a) if a.get() + amount <= capacity => {
                    *a.get_mut() = a.get() + amount;
                }
                _ => {
                    return Err(Error::PerAccountCapExceeded {
                        requested: amount,
                        capacity,
                    });
                }
            };
        }

        Ok(())
    }

    pub fn revert_changes(&self, addr: IpAddr, pubkey: Pubkey, amount: f64) -> Result<(), Error> {
        let mut cache = self.cache.lock().map_err(|e| {
            error!("Can not lock cache: {e:?}");
            Error::FailedToProcessTransaction
        })?;

        cache
            .addr_cache
            .entry(addr)
            .and_modify(|total| *total -= amount);

        cache
            .pubkey_cache
            .entry(pubkey)
            .and_modify(|total| *total -= amount);
        Ok(())
    }
}

fn deserialize_pubkey<'de, D>(deserializer: D) -> Result<Pubkey, D::Error>
where
    D: Deserializer<'de>,
{
    let pubkey = String::deserialize(deserializer)?;

    Pubkey::from_str(&pubkey).map_err(|e| {
        error!("invalid input, expect valid solana pubkey: {e:?}");
        de::Error::custom("invalid input, expect valid solana pubkey".to_string())
    })
}

fn deserialize_email<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let email = String::deserialize(deserializer)?;

    if !email_address::EmailAddress::is_valid(&email) {
        error!("invalid input, expect valid email address");
        Err(de::Error::custom("invalid email address".to_string()))
    } else {
        Ok(email)
    }
}

fn serialize_signature<S>(sig: &Signature, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    sig.to_string().serialize(serializer)
}

async fn get_recent_blockhash(state: &State) -> Result<Hash, Error> {
    state
        .solana_client
        .get_latest_blockhash()
        .await
        .map_err(|e| {
            error!("Failed to get recent blockhash: {e:?}");
            Error::FailedToProcessTransaction
        })
}

pub async fn get_or_create_ata(state: &State, wallet: &Pubkey) -> Result<Pubkey, Error> {
    let token_account = spl_associated_token_account::get_associated_token_address(
        wallet,
        &state.config.mint_pubkey,
    );

    if account_exists(&state.solana_client, &token_account).await? {
        return Ok(token_account);
    }

    let instruction =
        spl_associated_token_account::instruction::create_associated_token_account_idempotent(
            &state.authority_keypair.pubkey(),
            wallet,
            &state.config.mint_pubkey,
            &spl_token::ID,
        );

    let recent_blockhash = get_recent_blockhash(state).await?;

    let mut transaction =
        Transaction::new_with_payer(&[instruction], Some(&state.authority_keypair.pubkey()));
    transaction.sign(&[&state.authority_keypair], recent_blockhash);

    let result = state
        .solana_client
        .send_and_confirm_transaction(&transaction)
        .await;

    result.map_err(|e| {
        error!("Failed to get send transaction: {e:?}");
        Error::FailedToProcessTransaction
    })?;

    Ok(token_account)
}

pub async fn fund_token_account(
    state: &State,
    token_account: &Pubkey,
    amount: f64,
) -> Result<Signature, Error> {
    let amount = (amount * 10u64.pow(state.token_decimals as u32) as f64).round() as u64;
    let instruction = spl_token::instruction::mint_to(
        &spl_token::ID,
        &state.config.mint_pubkey,
        token_account,
        &state.authority_keypair.pubkey(),
        &[&state.authority_keypair.pubkey()],
        amount,
    )
    .map_err(|e| {
        error!("Failed to create Transaction: {e:?}");
        Error::FailedToProcessTransaction
    })?;

    let recent_blockhash = get_recent_blockhash(state).await?;

    let mut transaction =
        Transaction::new_with_payer(&[instruction], Some(&state.authority_keypair.pubkey()));
    transaction.sign(&[&state.authority_keypair], recent_blockhash);

    let result = state.solana_client.send_transaction(&transaction).await;

    result.map_err(|e| {
        error!("Failed to get send transaction: {e:?}");
        Error::FailedToProcessTransaction
    })
}

pub async fn account_exists(solana_client: &RpcClient, pubkey: &Pubkey) -> Result<bool, Error> {
    match solana_client.get_account(pubkey).await {
        Ok(_) => Ok(true),
        Err(client_error) => match client_error.kind {
            ClientErrorKind::RpcError(RpcError::ForUser(s)) if s.contains("AccountNotFound") => {
                Ok(false)
            }
            e => {
                error!("Failed to check account existence: {e:?}");
                Err(Error::FailedToProcessTransaction)
            }
        },
    }
}
