mod upload_function;

use anchor_client::solana_sdk::{pubkey::Pubkey, signer::Signer};
use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine};
use serde::{Deserialize, Serialize};

use mu_stack::StackID;

const PUBLIC_KEY_HEADER_NAME: &str = "X-MU-PUBLIC-KEY";
const SIGNATURE_HEADER_NAME: &str = "X-MU-SIGNATURE";

#[derive(Serialize)]
enum Subject {
    User(Pubkey),
    Stack(StackID),
}

#[derive(Serialize)]
struct Request {
    request: String,
    subject: Subject,
    params: serde_json::Value,
}

pub trait ApiRequest: Sized {
    type Response: Deserialize<'static>;
    type Error: Deserialize<'static>;

    fn make_request(&self) -> (Request, &dyn Signer);

    fn send(self, region_api_endpoint: String) -> Result<Result<Self::Response, Self::Error>> {
        let (payload, signer) = self.make_request();
        let payload_json = serde_json::to_vec(&payload)?;

        let pk_base64 = general_purpose::STANDARD.encode(signer.pubkey());

        let sig_payload = signer
            .try_sign_message(&payload_json)
            .context("Signing request payload")?;
        let sig_payload_base64 = general_purpose::STANDARD.encode(sig_payload);

        let http_client = reqwest::blocking::Client::new();

        let response = http_client
            .post(format!("{region_api_endpoint}/api"))
            .header(PUBLIC_KEY_HEADER_NAME, pk_base64)
            .header(SIGNATURE_HEADER_NAME, sig_payload_base64)
            .body(payload_json)
            .send()
            .context("Sending API request")?;

        response.json().context("Deserializing API response")
    }
}
