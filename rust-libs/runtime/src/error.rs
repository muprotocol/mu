use mu_stack::AssemblyID;
use thiserror::Error;
use wasmer::{CompileError, ExportError, InstantiationError, RuntimeError};
use wasmer_cache::SerializeError;
use wasmer_wasix::{WasiError, WasiRuntimeError};

//TODO: This enum is a mess, convert it to a struct with some kind and other fields to explain
//them.

#[derive(Error, Debug)]
pub enum Error {
    #[error("Function Runtime Error: {0:?}")]
    FunctionRuntimeError(FunctionRuntimeError),

    #[error("Function Loading Error: {0:?}")]
    FunctionLoadingError(FunctionLoadingError),

    #[error("Error in DB: {0:?}")]
    DBError(anyhow::Error),

    #[error("Failed to read message from function: {0:?}")]
    FailedToReadMessage(std::io::Error),

    #[error("Internal error: {0}")]
    Internal(anyhow::Error),

    #[error("Function didn't terminate cleanly")]
    FunctionDidntTerminateCleanly,

    #[error("Function reached instruction count limit")]
    Timeout,

    #[error("Failed to setup runtime cache: {0:?}")]
    CacheSetup(std::io::Error),

    #[error("The runtime was shut down")]
    RuntimeIsShutDown,
}

#[derive(Error, Debug)]
pub enum FunctionRuntimeError {
    #[error("Function reported fatal error: {0}")]
    FatalError(String),

    #[error("Function maximum memory exceeded")]
    MaximumMemoryExceeded,

    #[error("Function initialization failed: {0:?}")]
    FunctionInitializationFailed(RuntimeError),

    #[error("_start function is missing: {0:?}")]
    MissingStartFunction(ExportError),

    #[error("Failed to serialize message: {0:?}")]
    SerializationError(std::io::Error),
}
#[derive(Error, Debug)]
pub enum FunctionLoadingError {
    #[error("Can not find assembly with id: {0:?}")]
    AssemblyNotFound(AssemblyID),

    #[error("Invalid assembly definition: {0}")]
    InvalidAssemblyDefinition(String),

    #[error("WASM module for assembly {0:?} is corrupted or invalid")]
    InvalidAssembly(AssemblyID),

    #[error("Failed to build Wasi Env: {0:?}")]
    FailedToBuildWasmEnv(WasiRuntimeError),

    #[error("Failed to get Wasi import object: {0:?}")]
    FailedToGetImportObject(WasiError),

    #[error("Failed to instantiate wasm module: {0:?}")]
    FailedToInstantiateWasmModule(Box<InstantiationError>),

    #[error("Failed to get memory: {0:?}")]
    FailedToGetMemory(ExportError),

    #[error("Function requested memory size is too big")]
    RequestedMemorySizeTooBig,

    #[error("Failed to compile wasm module: {0:?}")]
    CompileWasmModule(CompileError),

    #[error("Failed to serialize cached wasm module: {0:?}")]
    SerializeCachedWasmModule(SerializeError),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
