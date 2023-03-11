#[cfg(feature = "client")]
mod client;

mod error;
pub mod requests;

use base64::{engine::general_purpose, Engine};
use log::error;
use mu_stack::{stack_id_as_string_serialization, StackID};
use requests::UploadFunctionRequest;
use serde::{Deserialize, Serialize};
use solana_sdk::{pubkey::Pubkey, signer::Signer};

pub use error::Error;

pub const SUBJECT_HEADER_NAME: &str = "X-MU-SUBJECT";
pub const SIGNATURE_HEADER_NAME: &str = "X-MU-SIGNATURE";

#[derive(Serialize, Deserialize)]
pub enum Subject {
    User(Pubkey),

    Stack {
        #[serde(serialize_with = "stack_id_as_string_serialization::serialize")]
        #[serde(deserialize_with = "stack_id_as_string_serialization::deserialize")]
        id: StackID,
        owner: Pubkey,
    },
}

impl Subject {
    pub fn encode_base64(&self) -> Result<String, Error> {
        let subject_json = serde_json::to_vec(&self).map_err(|e| {
            error!("Failed to serialize request subject: {e:?}");
            Error::SerializeSubject
        })?;

        Ok(general_purpose::STANDARD.encode(subject_json))
    }

    pub fn decode_base64<T: AsRef<[u8]>>(input: T) -> Result<Self, Error> {
        let subject_json = general_purpose::STANDARD.decode(input).map_err(|e| {
            error!("Failed to deserialize request subject: {e:?}");
            Error::SerializeSubject
        })?;

        serde_json::from_slice(&subject_json).map_err(|e| {
            error!("Failed to deserialize request subject: {e:?}");
            Error::SerializeSubject
        })
    }

    pub fn pubkey(&self) -> &Pubkey {
        match self {
            Subject::User(p) => p,
            Subject::Stack { owner, .. } => owner,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub enum Request {
    Ping,
    UploadFunction(UploadFunctionRequest),
}

pub struct SignedRequest {
    pub signature: String,
    pub subject: String,
    pub body: Vec<u8>,
}

impl Request {
    pub fn into_signed(
        &self,
        subject: Subject,
        signer: &dyn Signer,
    ) -> Result<SignedRequest, Error> {
        let body_json = serde_json::to_vec(self).map_err(|e| {
            error!("Failed to serialize request: {e:?}");
            Error::SerializeRequest
        })?;

        let sig_payload = signer.try_sign_message(&body_json).map_err(|e| {
            error!("Failed to sign request payload: {e:?}");
            Error::SignRequest
        })?;
        let sig_payload_base64 = general_purpose::STANDARD.encode(sig_payload);

        Ok(SignedRequest {
            signature: sig_payload_base64,
            body: body_json,
            subject: subject.encode_base64()?,
        })
    }
}
