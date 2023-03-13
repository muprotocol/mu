use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct UploadFunctionRequest {
    pub bytes: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UploadFunctionResponse {
    pub file_id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EchoRequest {
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EchoResponse {
    pub message: String,
}
