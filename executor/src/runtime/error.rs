//TODO
#![allow(dead_code)]

use super::types::{FunctionID, InstanceID};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Can not find function with id {0:?}")]
    FunctionNotFound(FunctionID),

    #[error("Can not convert input message to {0}")]
    IncorrectInputMessage(&'static str),

    #[error("Can parse {0} from convert output message")]
    IncorrectOutputMessage(&'static str),

    #[error("Function exited early: {0}")]
    FunctionEarlyExit(InstanceID),

    #[error("Function exited early: {0}")]
    FunctionAborted(InstanceID),
}
