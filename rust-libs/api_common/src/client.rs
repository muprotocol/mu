use std::{path::PathBuf, rc::Rc};

use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine};
use solana_sdk::signer::Signer;

use crate::{
    requests::{UploadFunctionRequest, UploadFunctionResponse},
    ApiRequestTemplate, ClientError, Error, Request, Response, SIGNATURE_HEADER_NAME,
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

        let (request, sign) = Request::UploadFunction(UploadFunctionRequest {
            bytes: general_purpose::STANDARD.encode(bytes),
        })
        .sign(&*user)?;

        match self.send(request, sign).context("Send request")? {
            Ok(Response::UploadFunction(resp)) => Ok(resp),
            Ok(_) => Err(ClientError::UnexpectedResponse("UploadFunction".into()).into()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn echo(&self, message: String, user: Box<dyn Signer>) -> Result<String> {
        let (request, sign) = Request::Echo(message).sign(&*user)?;

        match self.send(request, sign).context("Send request")? {
            Ok(Response::Echo(resp)) => Ok(resp),
            Ok(_) => Err(ClientError::UnexpectedResponse("Echo".into()).into()),
            Err(e) => Err(e.into()),
        }
    }

    fn send(&self, request: ApiRequestTemplate, sign: String) -> Result<Result<Response, Error>> {
        let request = self
            .client
            .post(&self.region_api_endpoint)
            .header(SIGNATURE_HEADER_NAME, sign)
            .body(serde_json::to_string(&request)?);

        let resp: Result<Response, Error> =
            request.send().context("Sending API request")?.json()?;
        Ok(resp)
    }
}
