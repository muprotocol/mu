use super::types::Key;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum Error {
    // user error
    #[error("mudb_error> database {0} already exist")]
    DbAlreadyExist(String),
    #[error("mudb_error> database {0} dose not exist")]
    DbDoseNotExist(String),
    #[error("mudb_error> table {0} already exist")]
    TableAlreadyExist(String),
    #[error("mudb_error> table {0} dose not exist")]
    TableDoseNotExist(String),
    #[error("mudb_error> table {0} is reserved")]
    TableIsReserved(String),
    #[error("mudb_error> key {0} already exist")]
    KeyAlreadyExist(Key),
    #[error("mudb_error> invalid_table_name> {0} {1}")]
    InvalidTableName(String, String),
    #[error("mudb_error> invalid_json_command> {0}")]
    InvalidJsonCommand(String),

    // outer error
    #[error("mudb_error> sled> {0}")]
    Sled(sled::Error),
    #[error("mudb_error> sled_transaction> {0}")]
    SledTrans(sled::transaction::TransactionError),
    #[error("mudb_error> serde_json> {0}")]
    SerdeJson(String),
    #[error("mudb_error> command was cancelled")]
    CommandCancelled,
    #[error("mudb_error> command panicked")]
    CommandPanicked,
    #[error("mudb_error> invalid_database_id> {0}")]
    InvalidDbId(String),
    #[error("mudb_error> manager_mailbox> {0}")]
    ManagerMailBox(ManagerMailBoxError),
    #[error("Can not stop manager")]
    FailedToStopManager,
}

impl Eq for Error {}

impl From<sled::Error> for Error {
    fn from(err: sled::Error) -> Self {
        Self::Sled(err)
    }
}

impl From<sled::transaction::TransactionError> for Error {
    fn from(err: sled::transaction::TransactionError) -> Self {
        Self::SledTrans(err)
    }
}

impl From<serde_json::error::Error> for Error {
    fn from(err: serde_json::error::Error) -> Self {
        Self::SerdeJson(err.to_string())
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

// impl From<mailbox_processor::Error> for Error {
//     fn from(err: mailbox_processor::Error) -> Self {
//         Self::ManagerErr(err)
//     }
// }

#[derive(Clone, Error, Debug, PartialEq, Eq)]
pub enum JsonCommandError {
    #[error("invalid operation")]
    InvalidOpr,
    #[error("expect number")]
    ExpectNum,
    #[error("expect array")]
    ExpectArr,
    #[error("expect object")]
    ExpectObj,
    #[error("expect string")]
    ExpectStr,
}

impl From<JsonCommandError> for Error {
    fn from(err: JsonCommandError) -> Self {
        Error::InvalidJsonCommand(err.to_string())
    }
}

#[derive(Error, Debug, PartialEq)]
pub enum ManagerMailBoxError {
    #[error("create_db> {0}")]
    CreateDb(mailbox_processor::Error),
    #[error("drop_db> {0}")]
    DropDb(mailbox_processor::Error),
    #[error("get_db> {0}")]
    GetDb(mailbox_processor::Error),
    #[error("get_cache> {0}")]
    GetCache(mailbox_processor::Error),
    #[error("Failed to stop manager")]
    Stop(mailbox_processor::Error),
}

impl From<ManagerMailBoxError> for Error {
    fn from(err: ManagerMailBoxError) -> Self {
        Self::ManagerMailBox(err)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
pub type JsonCommandResult<T> = std::result::Result<T, JsonCommandError>;
