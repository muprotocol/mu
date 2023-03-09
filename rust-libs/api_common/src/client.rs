use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose, Engine};
use solana_sdk::signer::Signer;
use uuid::Uuid;

use crate::{
    request::UploadFunctionRequest, ApiRequest, PUBLIC_KEY_HEADER_NAME, SIGNATURE_HEADER_NAME,
};

//TODO: support async clients too
pub struct ApiClient {
    region_api_endpoint: String,
    client: reqwest::blocking::Client,
}

impl ApiClient {
    pub fn new<S: AsRef<str>>(region_base_url: S) -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
            region_api_endpoint: format!("{}/api", region_base_url.as_ref()),
        }
    }

    pub fn upload_function(&self, file_path: PathBuf, user: Box<dyn Signer>) -> Result<Uuid> {
        let bytes = std::fs::read(file_path).context("Reading function wasm module")?;
        let file_id = Uuid::new_v4();

        let request = UploadFunctionRequest {
            user,
            bytes,
            file_id,
        };

        if let Err(e) = self.send(request).context("Upload function wasm module")? {
            Err(anyhow!("Failed to upload function: {e}"))
        } else {
            Ok(file_id)
        }
    }

    fn send<R: ApiRequest>(&self, request: R) -> Result<Result<R::Response, R::Error>> {
        let (payload, signer) = request.make_request();
        let payload_json = serde_json::to_vec(&payload)?;

        let pk_base64 = general_purpose::STANDARD.encode(signer.pubkey());

        let sig_payload = signer
            .try_sign_message(&payload_json)
            .context("Signing request payload")?;
        let sig_payload_base64 = general_purpose::STANDARD.encode(sig_payload);

        let response = self
            .client
            .post(&self.region_api_endpoint)
            .header(PUBLIC_KEY_HEADER_NAME, pk_base64)
            .header(SIGNATURE_HEADER_NAME, sig_payload_base64)
            .body(payload_json)
            .send()
            .context("Sending API request")?;

        response.json().context("Deserializing API response")
    }
}
