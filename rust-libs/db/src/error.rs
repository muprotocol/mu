use super::types::Key;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("muDB: TiKV client error: {0}")]
    TikvErr(#[from] tikv_client::Error),
    #[error("muDB: cant deserialize Key: {0}")]
    CantDeserializeKey(String),
    #[error("muDB: stack_id or table doesn't exist: {0:?}")]
    StackIdOrTableDoseNotExist(Key),
    #[error("muDB: TiKV startup timeout: {0}, so rest of the processes killed")]
    TikvConnectionTimeout(String),
}

pub type Result<T> = std::result::Result<T, Error>;
