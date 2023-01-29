use super::types::Key;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("mu_db: TiKV client error: {0}")]
    TikvErr(#[from] tikv_client::Error),
    #[error("mu_db: TiKV startup timeout: {0}, so rest of the processes killed")]
    TikvConnectionTimeout(String),
    #[error("mu_db: cant deserialize Key: {0}")]
    CantDeserializeKey(String),
    #[error("mu_db: stack_id or table doesn't exist: {0:?}")]
    StackIdOrTableDoseNotExist(Key),
    #[error("mu_db: internal error: {0}")]
    InternalErr(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
