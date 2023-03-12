use serde::{Deserialize, Serialize};

#[derive(thiserror::Error, Debug, Serialize, Deserialize)]
pub enum Error {
    #[error("Server Error")]
    ServerError(ServerError),

    #[error("Client Error")]
    ClientError(ClientError),

    #[error("Failed to serialize subject")]
    SerializeSubject,

    #[error("Failed to serialize request")]
    SerializeRequest,

    #[error("Failed to sign request")]
    SignRequest,
}

#[derive(thiserror::Error, Debug, Serialize, Deserialize)]
pub enum ServerError {
    #[error("Failed to upload function source")]
    UploadFunction,

    #[error("Unexpected subject type, expected {0}, got {1}")]
    UnexpectedSubject(String, String),

    #[error("Failed to Serialize response")]
    FailedToSerializeResponse,

    #[error("BadRequest: {0}")]
    BadRequest(String),
}

#[derive(thiserror::Error, Debug, Serialize, Deserialize)]
pub enum ClientError {
    #[error("Unexpected resposne type, expected {0}")]
    UnexpectedResponse(String),
}

impl From<ClientError> for Error {
    fn from(value: ClientError) -> Self {
        Self::ClientError(value)
    }
}

impl From<ServerError> for Error {
    fn from(value: ServerError) -> Self {
        Self::ServerError(value)
    }
}
