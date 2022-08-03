use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Can not find function with id {0}")]
    FunctionNotFound(Uuid),

    #[error("Can not convert input message to {0}")]
    IncorrectInputMessage(&'static str),

    #[error("Can parse {0} from convert output message")]
    IncorrectOutputMessage(&'static str),
}
