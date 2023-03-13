use std::{path::PathBuf, rc::Rc};

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose, Engine};
use mu_stack::StackOwner;
use solana_sdk::signer::Signer;

use crate::{
    requests::{EchoRequest, EchoResponse, UploadFunctionRequest, UploadFunctionResponse},
    sign_request, SIGNATURE_HEADER_NAME,
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

    pub fn upload_function(&self, file_path: PathBuf, signer: Rc<dyn Signer>) -> Result<String> {
        let bytes = std::fs::read(file_path).context("Reading function wasm module")?;
        let request = UploadFunctionRequest {
            bytes: general_purpose::STANDARD.encode(bytes),
        };

        let (request_body, sign) = sign_request(
            request,
            "upload_function".to_string(),
            Some(StackOwner::Solana(signer.pubkey().to_bytes())),
            signer,
        )?;

        let response: UploadFunctionResponse =
            serde_json::from_slice(&self.send(request_body, sign)?)?;

        Ok(response.file_id)
    }

    pub fn echo(&self, message: String, signer: Rc<dyn Signer>) -> Result<String> {
        let request = EchoRequest { message };

        let (request_body, sign) = sign_request(request, "echo".to_string(), None, signer)?;

        let response: EchoResponse = serde_json::from_slice(&self.send(request_body, sign)?)?;
        Ok(response.message)
    }

    fn send(&self, request: Vec<u8>, sign: String) -> Result<bytes::Bytes> {
        let request = self
            .client
            .post(&self.region_api_endpoint)
            .header(SIGNATURE_HEADER_NAME, sign)
            .body(request);

        let resp = request.send().context("Sending API request")?;

        if resp.status().is_success() {
            resp.bytes().context("")
        } else {
            bail!("Api Error: {}", resp.text()?)
        }
    }
}
