use std::{path::PathBuf, rc::Rc};

use anyhow::{Context, Result};
use solana_sdk::signer::Signer;

use crate::{
    requests::{UploadFunctionRequest, UploadFunctionResponse},
    Error, Request, SignedRequest, Subject, SIGNATURE_HEADER_NAME, SUBJECT_HEADER_NAME,
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

    pub fn upload_function(
        &self,
        file_path: PathBuf,
        user: Rc<dyn Signer>,
    ) -> Result<UploadFunctionResponse> {
        let bytes = std::fs::read(file_path).context("Reading function wasm module")?;

        let subject = Subject::User(user.pubkey());

        let request = Request::UploadFunction(UploadFunctionRequest { bytes })
            .into_signed(subject, &*user)?;

        match self.send(request).context("Send request")? {
            Ok(ref bytes) => serde_json::from_slice(bytes).context("can't deserialize response"),
            Err(e) => Err(e.into()),
        }
    }

    pub fn ping(&self, user: Box<dyn Signer>) -> Result<()> {
        let subject = Subject::User(user.pubkey());
        let request = Request::Ping.into_signed(subject, &*user)?;

        match self.send(request).context("Send request")? {
            Ok(ref bytes) => serde_json::from_slice(bytes).context("can't deserialize response"),
            Err(e) => Err(e.into()),
        }
    }

    fn send(&self, request: SignedRequest) -> Result<Result<bytes::Bytes, Error>> {
        let response = self
            .client
            .post(&self.region_api_endpoint)
            .header(SUBJECT_HEADER_NAME, request.subject)
            .header(SIGNATURE_HEADER_NAME, request.signature)
            .body(request.body)
            .send()
            .context("Sending API request")?;

        Ok(if response.status().is_success() {
            Ok(response.bytes()?)
        } else {
            Err(response.json()?)
        })
    }
}
