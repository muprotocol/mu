use std::path::PathBuf;

use anchor_client::solana_sdk::signer::Signer;
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose, Engine};
use serde_json::json;
use uuid::Uuid;

use super::{ApiRequest, Request, Subject};

pub(super) struct UploadFunctionRequest {
    file_id: Uuid,
    user: Box<dyn Signer>,
    bytes: Vec<u8>,
}

impl ApiRequest for UploadFunctionRequest {
    type Response = ();
    type Error = String;

    fn make_request(&self) -> (Request, &dyn Signer) {
        (
            Request {
                request: "upload_function".into(),
                subject: Subject::User(self.user.pubkey()),
                params: json! ({
                        "file_id": self.file_id.to_string(),
                        "bytes": general_purpose::STANDARD.encode(self.bytes),
                }),
            },
            &*self.user,
        )
    }
}

pub fn upload_function(
    file_path: PathBuf,
    user: Box<dyn Signer>,
    region_base_url: String,
) -> Result<Uuid> {
    let bytes = std::fs::read(file_path).context("Reading function wasm module")?;
    let file_id = Uuid::new_v4();

    if let Err(e) = (UploadFunctionRequest {
        user,
        bytes,
        file_id,
    })
    .send(region_base_url)
    .context("Upload function wasm module")
    {
        bail!("Upload failed: {e}");
    };

    Ok(file_id)
}
