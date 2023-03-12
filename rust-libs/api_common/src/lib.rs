#[cfg(feature = "client")]
pub mod client;
mod error;
pub mod requests;

use base64::{engine::general_purpose, Engine};
use log::error;
use requests::{UploadFunctionRequest, UploadFunctionResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;
use solana_sdk::signer::Signer;

pub use error::{ClientError, Error, ServerError};

pub const SIGNATURE_HEADER_NAME: &str = "X-MU-SIGNATURE";

#[derive(Serialize, Deserialize, Debug)]
pub enum Request {
    Echo(String),
    UploadFunction(UploadFunctionRequest),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ApiRequestTemplate {
    pub request: String,
    pub user: String,
    pub params: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ApiResponseTemplate {
    params: serde_json::Value,
}

impl Request {
    pub fn sign(&self, user: &dyn Signer) -> Result<(ApiRequestTemplate, String), Error> {
        let body_json = serde_json::to_vec(self).map_err(|e| {
            error!("Failed to serialize request: {e:?}");
            Error::SerializeRequest
        })?;

        let sig_payload = user.try_sign_message(&body_json).map_err(|e| {
            error!("Failed to sign request payload: {e:?}");
            Error::SignRequest
        })?;
        let sig_payload_base64 = general_purpose::STANDARD.encode(sig_payload);

        let (request_type, params) = match self {
            Request::Echo(m) => ("echo", json!({ "message": m })),
            Request::UploadFunction(r) => (
                "upload_function",
                json!(
                {
                    "bytes": r.bytes
                }
                ),
            ),
        };

        Ok((
            ApiRequestTemplate {
                request: request_type.to_string(),
                user: user.pubkey().to_string(),
                params,
            },
            sig_payload_base64,
        ))
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Response {
    Echo(String),
    UploadFunction(UploadFunctionResponse),
}
