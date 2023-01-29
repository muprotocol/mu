//TODO
#![allow(dead_code)]

use mu_stack::AssemblyID;
use thiserror::Error;
use wasmer::{ExportError, InstantiationError, RuntimeError};
use wasmer_wasi::{WasiError, WasiStateCreationError};

#[derive(Error, Debug)]
pub enum Error {
    #[error("Function Runtime Error: {0:?}")]
    FunctionRuntimeError(FunctionRuntimeError),

    #[error("Function Loading Error: {0:?}")]
    FunctionLoadingError(FunctionLoadingError),

    #[error("Error in DB")]
    DBError(&'static str),

    #[error("Failed to read message from function: {0:?}")]
    FailedToReadMessage(std::io::Error),

    #[error("Internal error: {0}")]
    Internal(anyhow::Error),

    #[error("Function didn't terminate cleanly")]
    FunctionDidntTerminateCleanly,
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
    SerializationError(std::io::Error),
}
#[derive(Error, Debug)]
pub enum FunctionLoadingError {
    #[error("Can not find assembly with id {0:?}")]
    AssemblyNotFound(AssemblyID),

    #[error("WASM module for assembly {0:?} is corrupted or invalid ")]
    InvalidAssembly(AssemblyID),

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
