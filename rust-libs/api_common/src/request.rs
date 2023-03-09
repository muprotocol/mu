use base64::{engine::general_purpose, Engine};
use serde_json::json;
use solana_sdk::signer::Signer;
use uuid::Uuid;

use super::{ApiRequest, Request, Subject};

pub struct UploadFunctionRequest {
    pub file_id: Uuid,
    pub user: Box<dyn Signer>,
    pub bytes: Vec<u8>,
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
                        "bytes": general_purpose::STANDARD.encode(&self.bytes),
                }),
            },
            &*self.user,
        )
    }
}
