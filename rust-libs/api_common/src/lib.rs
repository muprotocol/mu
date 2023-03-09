#[cfg(feature = "client")]
mod client;

mod request;

use mu_stack::{stack_id_as_string_serialization, StackID};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use solana_sdk::{pubkey::Pubkey, signer::Signer};

pub const PUBLIC_KEY_HEADER_NAME: &str = "X-MU-PUBLIC-KEY";
pub const SIGNATURE_HEADER_NAME: &str = "X-MU-SIGNATURE";

#[derive(Serialize, Deserialize)]
pub enum Subject {
    User(Pubkey),

    #[serde(serialize_with = "stack_id_as_string_serialization::serialize")]
    #[serde(deserialize_with = "stack_id_as_string_serialization::deserialize")]
    Stack(StackID),
}

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub request: String,
    pub subject: Subject,
    pub params: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
pub struct Response {
    pub params: serde_json::Value,
}

pub trait ApiRequest: Sized {
    type Response: DeserializeOwned + Serialize;
    type Error: DeserializeOwned + Serialize;

    fn make_request(&self) -> (Request, &dyn Signer);
}
