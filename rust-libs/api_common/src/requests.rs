use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct UploadFunctionRequest {
    pub bytes: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UploadFunctionResponse {
    pub file_id: String,
}
