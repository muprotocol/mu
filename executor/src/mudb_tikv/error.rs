use super::types::Key;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("mudb_error -> tikv error -> {0}")]
    TikvErr(tikv_client::Error),
    #[error("mudb_error -> cant deserialize key -> {0}")]
    CantDeserializeKey(String),
    #[error("mudb_error -> stack_id or table doesn't exist -> {0:?}")]
    StackIdOrTableDoseNotExist(Key),
    #[error("mudb_error -> embedding tikv error -> {0}")]
    EmbeddingTikvErr(String),
}

impl From<tikv_client::Error> for Error {
    fn from(te: tikv_client::Error) -> Self {
        Self::TikvErr(te)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
