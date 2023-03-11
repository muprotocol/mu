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
    UploadFunctionError,
}

#[derive(thiserror::Error, Debug, Serialize, Deserialize)]
pub enum ClientError {}

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
