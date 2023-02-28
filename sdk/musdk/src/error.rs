use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Unexpected message kind, first message must be an ExecuteFunction request")]
    UnexpectedFirstMessageKind,

    #[error("Unknown incoming message code {0}")]
    UnknownIncomingMessageCode(u16),

    #[error("Failed to deserialize incoming message: {0}")]
    CannotDeserializeIncomingMessage(std::io::Error),

    #[error("Failed to serialize outgoing message: {0}")]
    CannotSerializeOutgoingMessage(std::io::Error),

    #[error("Unknown function {0}")]
    UnknownFunction(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Unexpected message kind, was expecting {0}")]
    UnexpectedMessageKind(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;
