use std::sync::Arc;

use crate::{memory::create_memory, Error, FunctionLoadingError, Result, Usage};

use wasmer::{CompilerConfig, Store};
use wasmer_compiler_llvm::LLVM;
use wasmer_middlewares::Metering;

#[inline]
pub fn create_store(
    memory_limit: byte_unit::Byte,
    giga_instructions_limit: Option<u32>,
) -> Result<Store> {
    let mut compiler_config = LLVM::default();

    let metering_points = giga_instructions_limit.unwrap_or(u32::MAX) as u64 * 1_000_000_000;

    let metering = Arc::new(Metering::new(metering_points, |_| 1));
    compiler_config.push_middleware(metering);

    let memory = create_memory(memory_limit).map_err(|_| {
        Error::FunctionLoadingError(FunctionLoadingError::RequestedMemorySizeTooBig)
    })?;

    Ok(Store::new_with_tunables(compiler_config, memory))
}

#[inline]
pub fn create_usage(
    db_read: u64,
    db_write: u64,
    instructions_count: u64,
    memory: byte_unit::Byte,
) -> Usage {
    let memory_megabytes = memory
        .get_adjusted_unit(byte_unit::ByteUnit::MB)
        .get_value();
    let memory_megabytes = memory_megabytes.floor() as u64;

    Usage {
        db_strong_reads: 0,
        db_strong_writes: 0,
        db_weak_reads: db_read,
        db_weak_writes: db_write,
        function_instructions: instructions_count,
        memory_megabytes,
    }
}
