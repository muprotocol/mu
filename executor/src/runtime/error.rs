//TODO
#![allow(dead_code)]

use super::types::FunctionID;
use thiserror::Error;
use wasmer::{ExportError, InstantiationError, RuntimeError};
use wasmer_wasi::{WasiError, WasiStateCreationError};

#[derive(Error, Debug)]
pub enum Error {
    #[error("Function Runtime Error: {0:?}")]
    FunctionRuntimeError(FunctionRuntimeError),

    #[error("Function Loading Error: {0:?}")]
    FunctionLoadingError(FunctionLoadingError),

    #[error("Can not convert input message to {0}")]
    IncorrectInputMessage(&'static str),

    #[error("Can parse {0} from convert output message")]
    IncorrectOutputMessage(&'static str),

    #[error("Invalid message type: {0}")]
    InvalidMessageType(String),

    #[error("Message Deserialization failed: {0}")]
    MessageDeserializationFailed(serde_json::Error),

    #[error("Message Serialization failed: {0}")]
    MessageSerializationFailed(serde_json::Error),

    #[error("Message id can not be None")]
    MessageIDIsNone,

    #[error("Error in DB")]
    DBError(&'static str),

    #[error("Internal error: {0}")]
    Internal(anyhow::Error),
}

#[derive(Error, Debug)]
pub enum FunctionRuntimeError {
    #[error("Function reported fatal error: {0}")]
    FatalError(String),

    #[error("Function exited early: {0}")]
    FunctionEarlyExit(RuntimeError),

    #[error("Function maximum memory exceeded")]
    MaximumMemoryExceeded,

    #[error("Function initialization failed: {0}")]
    FunctionInitializationFailed(RuntimeError),

    #[error("_start function is missing: {0}")]
    MissingStartFunction(ExportError),

    #[error("Failed to serialize message: {0}")]
    SerializtionError(std::io::Error),
}
#[derive(Error, Debug)]
pub enum FunctionLoadingError {
    #[error("Can not find function with id {0:?}")]
    FunctionNotFound(FunctionID),

    #[error("Function {0:?} wasm module is corrupted or invalid ")]
    InvalidFunctionModule(FunctionID),

    #[error("Failed to build Wasi Env: {0}")]
    FailedToBuildWasmEnv(WasiStateCreationError),

    #[error("Failed to get Wasi import object: {0}")]
    FailedToGetImportObject(WasiError),

    #[error("Failed to instantiate wasm module: {0}")]
    FailedToInstantiateWasmModule(InstantiationError),

    #[error("Failed to get memory: {0}")]
    FailedToGetMemory(ExportError),

    #[error("Function requested memory size is too big")]
    RequestedMemorySizeTooBig,
}
