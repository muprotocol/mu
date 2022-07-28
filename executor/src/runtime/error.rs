#[allow(unused)]
use uuid::Uuid;
use wasmer::{CompileError, ExportError, InstantiationError, RuntimeError};
use wasmer_wasi::{WasiError, WasiStateCreationError};

#[derive(Debug)]
pub enum Error {
    IOError(std::io::Error),
    FunctionNotFound(Uuid),
    CompileError(CompileError),
    WasiStateCreationError(WasiStateCreationError),
    WasiError(WasiError),
    InstantiationError(InstantiationError),
    ExportError(ExportError),
    RuntimeError(RuntimeError),
}

pub type Result<T> = std::result::Result<T, Error>;
