use super::input::Key;
use thiserror::Error;

// TODO: encapsolate some error into internal error.
#[derive(Error, Debug)]
pub enum Error {
    // user error
    #[error("table {0} already exist")]
    TableAlreadyExist(String),
    #[error("table {0} dose not exist")]
    TableDoseNotExist(String),
    #[error("table {0} is reserved")]
    TableIsReserved(String),
    #[error("trying to set key, {0}, while it's auto increment")]
    TryingToSetKeyWhileItIsAutoIncrement(String),
    #[error("trying to insert item with no key")]
    TryingToInsertItemWithNoKey,
    #[error("key {0} already exist")]
    KeyAlreadyExist(Key),
    #[error("validation errors: {0}")]
    InputValidationErr(validator::ValidationErrors),

    // internal error
    #[error("sled error: {0}")]
    SledErr(sled::Error),
    #[error("sled transaction error: {0}")]
    SledTransErr(sled::transaction::TransactionError),
    #[error("serde_json error")]
    SerdeJsonErr(serde_json::Error), // (serde_json::error::Error)
    #[error("command was cancelled")]
    CommandCancelled,
    #[error("command panicked")]
    CommandPanicked,
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Error::TableAlreadyExist(l), Error::TableAlreadyExist(r)) => l == r,
            (Error::TableDoseNotExist(l), Error::TableDoseNotExist(r)) => l == r,
            (Error::TableIsReserved(l), Error::TableIsReserved(r)) => l == r,
            (
                Error::TryingToSetKeyWhileItIsAutoIncrement(l),
                Error::TryingToSetKeyWhileItIsAutoIncrement(r),
            ) => l == r,
            (Error::TryingToInsertItemWithNoKey, Error::TryingToInsertItemWithNoKey) => true,
            (Error::KeyAlreadyExist(l), Error::KeyAlreadyExist(r)) => l == r,
            (Error::InputValidationErr(l), Error::InputValidationErr(r)) => l == r,
            (Error::SledErr(l), Error::SledErr(r)) => l == r,
            (Error::SledTransErr(l), Error::SledTransErr(r)) => l == r,
            (Error::SerdeJsonErr(l), Error::SerdeJsonErr(r)) => {
                l.line() == r.line() && l.column() == r.column()
            }
            (Error::CommandCancelled, Error::CommandCancelled) => true,
            (Error::CommandPanicked, Error::CommandPanicked) => true,
            _ => false,
        }
    }
}

impl Eq for Error {}

impl From<sled::Error> for Error {
    fn from(err: sled::Error) -> Self {
        Self::SledErr(err)
    }
}

impl From<sled::transaction::TransactionError> for Error {
    fn from(err: sled::transaction::TransactionError) -> Self {
        Self::SledTransErr(err)
    }
}

impl From<serde_json::error::Error> for Error {
    fn from(err: serde_json::error::Error) -> Self {
        // Self::SerdeJsonErr(err)
        Self::SerdeJsonErr(err)
    }
}

impl From<validator::ValidationErrors> for Error {
    fn from(err: validator::ValidationErrors) -> Self {
        Self::InputValidationErr(err)
    }
}

impl From<tokio::task::JoinError> for Error {
    fn from(err: tokio::task::JoinError) -> Self {
        if err.is_cancelled() {
            Error::CommandCancelled
        } else {
            // err.is_panic()
            Error::CommandPanicked
        }
    }
}

#[derive(Clone, Error, Debug, PartialEq, Eq)]
pub enum InvalidQueryError {
    #[error("InvalidOprErr")]
    InvalidOprErr,
    #[error("ExpectNumErr")]
    ExpectNumErr,
    #[error("ExpectArrErr")]
    ExpectArrErr,
    #[error("ExpectObjErr")]
    ExpectObjErr,
    #[error("ExpectStrErr")]
    ExpectStrErr,
}

impl From<InvalidQueryError> for validator::ValidationError {
    fn from(err: InvalidQueryError) -> Self {
        use validator::ValidationError;
        use InvalidQueryError::*;
        match err {
            InvalidOprErr => ValidationError::new("invalid query err: expected `$ operation`"),
            ExpectNumErr => ValidationError::new("invalid query err: expected `Number`"),
            ExpectArrErr => ValidationError::new("invalid query err: expected `Array`"),
            ExpectObjErr => ValidationError::new("invalid query err: expected `Object`"),
            ExpectStrErr => ValidationError::new("invalid query err: expected `String`"),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
pub type ValidationResult<T> = std::result::Result<T, validator::ValidationError>;
pub type QueryValidationResult<T> = std::result::Result<T, InvalidQueryError>;
