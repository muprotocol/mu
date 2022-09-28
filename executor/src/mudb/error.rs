use std::{
    fmt::{self, Display},
    ops::Deref,
};

use super::types::Key;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum Error {
    // user error
    #[error("mudb_error|> database already exist|> {0}")]
    DbAlreadyExist(String),
    #[error("mudb_error|> database dose not exist|> {0}")]
    DbDoseNotExist(String),
    #[error("mudb_error|> table already exist|> {0}")]
    TableAlreadyExist(String),
    #[error("mudb_error|> table dose not exist|> {0}")]
    TableDoseNotExist(String),
    #[error("mudb_error|> table name is invalid|> {0} {1}")]
    InvalidTableName(String, String),
    #[error("mudb_error|> json command is invalid|> {0}")]
    InvalidJsonCommand(String),
    #[error("mudb_error|> expected object value|> {0}")]
    ExpectedObjectValue(String),
    #[error("mudb_error|> missing index attribute|> {0}")]
    MissingIndexAttr(String),
    #[error("mudb_error|> index attribute should be a String|> {0}")]
    IndexAttrShouldBeString(String),
    #[error("mudb_error|> this attributes are indexed and can't update|> {0}")]
    IndexAttrCantUpdate(List<String>),
    #[error("mudb_error|> there is no index tree with name|> {0}")]
    HaveNoIndexTree(String),
    #[error("mudb_error|> secondary key already exist|> {0}")]
    SecondaryKeyAlreadyExist(String, Key),

    // outer error
    #[error("mudb_error|> sled|> {0}")]
    Sled(sled::Error),
    #[error("mudb_error|> sled transaction|> {0}")]
    SledTrans(sled::transaction::TransactionError),
    #[error("mudb_error|> serde json|> {0}")]
    SerdeJson(String),
    #[error("mudb_error|> command was cancelled")]
    CommandCancelled,
    #[error("mudb_error|> command panicked")]
    CommandPanicked,
    #[error("mudb_error|> database_id is invalid|> {0}")]
    InvalidDbId(String),
    #[error("mudb_error|> manager mailbox|> {0}")]
    ManagerMailBox(ManagerMailBoxError),
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

#[derive(Debug)]
pub struct List<T: PartialEq + ToString>(Vec<T>);

impl<T> Deref for List<T>
where
    T: PartialEq + ToString,
{
    type Target = Vec<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> PartialEq for List<T>
where
    T: PartialEq + ToString,
{
    fn eq(&self, other: &Self) -> bool {
        self.iter().all(|x| other.contains(x)) && self.len() == other.len()
    }
}

impl<T> Display for List<T>
where
    T: PartialEq + ToString,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let x = self
            .iter()
            .fold(String::new(), |acc, y| acc + &y.to_string() + " ,");
        write!(f, "[ {} ]", x)
    }
}

impl<T> From<Vec<T>> for List<T>
where
    T: PartialEq + ToString,
{
    fn from(x: Vec<T>) -> Self {
        Self(x)
    }
}

#[derive(Clone, Error, Debug, PartialEq, Eq)]
pub enum JsonCommandError {
    #[error("invalid operation|> {0}")]
    InvalidOpr(String),
    #[error("expect number|> {0}")]
    ExpectNum(String),
    #[error("expect array|> {0}")]
    ExpectArr(String),
    #[error("expect object|> {0}")]
    ExpectObj(String),
    #[error("expect string|> {0}")]
    ExpectStr(String),
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
}

impl From<ManagerMailBoxError> for Error {
    fn from(err: ManagerMailBoxError) -> Self {
        Self::ManagerMailBox(err)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
pub type JsonCommandResult<T> = std::result::Result<T, JsonCommandError>;
