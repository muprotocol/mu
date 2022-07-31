use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Can not find function with id {0}")]
    FunctionNotFound(Uuid),
}
