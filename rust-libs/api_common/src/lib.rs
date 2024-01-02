#[cfg(feature = "client")]
pub mod client;
mod error;
pub mod requests;

use std::str::FromStr;

use base64::{engine::general_purpose, Engine};
use log::error;
use mu_stack::StackOwner;
use pwr_rs::PrivateKey;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use error::{ClientError, Error, ServerError};

pub const SIGNATURE_HEADER_NAME: &str = "X-MU-SIGNATURE";

#[derive(Serialize, Deserialize, Debug)]
pub struct ApiRequestTemplate {
    pub request: String,
    pub params: serde_json::Value,

    #[serde(serialize_with = "serialize_stack_owner")]
    #[serde(deserialize_with = "deserialize_stack_owner")]
    pub user: Option<StackOwner>,
    // TODO: Stack ID
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ApiResponseTemplate {
    params: serde_json::Value,
}

pub fn sign_request<T: Serialize>(
    request: T,
    request_type: String,
    user: Option<StackOwner>,
    signer: &PrivateKey,
) -> Result<(Vec<u8>, String), Error> {
    let body = ApiRequestTemplate {
        request: request_type,
        user,
        params: serde_json::to_value(request).map_err(|e| {
            error!("Failed to serialize request: {e:?}");
            Error::SerializeRequest
        })?,
    };

    let body_json = serde_json::to_vec(&body).map_err(|e| {
        error!("Failed to serialize request: {e:?}");
        Error::SerializeRequest
    })?;

    let sig_payload = signer.sign_message(&body_json).map_err(|e| {
        error!("Failed to sign request payload: {e:?}");
        Error::SignRequest
    })?;
    let sig_payload_base64 = general_purpose::STANDARD.encode(sig_payload.1);

    Ok((body_json, sig_payload_base64))
}

pub fn serialize_stack_owner<S>(item: &Option<StackOwner>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match item {
        None => serializer.serialize_none(),
        Some(s) => {
            let s = s.to_string();
            serializer.serialize_some(&s)
        }
    }
}

pub fn deserialize_stack_owner<'de, D>(deserializer: D) -> Result<Option<StackOwner>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = Option::<String>::deserialize(deserializer)?;
    match s {
        Some(s) => Ok(Some(
            mu_stack::StackOwner::from_str(s.as_str())
                .map_err(|_| serde::de::Error::custom("invalid StackOwner"))?,
        )),
        None => Ok(None),
    }
}
