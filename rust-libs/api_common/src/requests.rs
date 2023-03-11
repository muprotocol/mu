use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct UploadFunctionRequest {
    pub bytes: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct UploadFunctionResponse {
    pub file_id: String,
}
