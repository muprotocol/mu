use std::collections::HashMap;

use super::{
    error::{Error, FunctionLoadingError, FunctionRuntimeError, Result},
    types::{FunctionHandle, FunctionIO},
};

use wasmer::{Instance, Module, Store};
use wasmer_middlewares::metering::{get_remaining_points, MeteringPoints};
use wasmer_wasix::{Pipe, WasiEnv};

pub fn start(
    mut store: Store,
    module: &Module,
    envs: HashMap<String, String>,
    giga_instructions_limit: Option<u32>,
) -> Result<FunctionHandle> {
    //TODO: Check wasi version specified in this module and if we can run it!

    let (stdin_tx, stdin_rx) = Pipe::channel();
    let (stdout_tx, stdout_rx) = Pipe::channel();
    let (stderr_tx, stderr_rx) = Pipe::channel();

    let program_name = module.name().unwrap_or("module");
    let wasi_env = WasiEnv::builder(program_name)
        .stdin(Box::new(stdin_rx))
        .stdout(Box::new(stdout_tx))
        .stderr(Box::new(stderr_tx))
        .envs(envs)
        .finalize(&mut store)
        .map_err(|e| Error::FunctionLoadingError(FunctionLoadingError::FailedToBuildWasmEnv(e)))?;

    let import_object = wasi_env.import_object(&mut store, module).map_err(|e| {
        Error::FunctionLoadingError(FunctionLoadingError::FailedToGetImportObject(e))
    })?;

    let instance = Instance::new(&mut store, module, &import_object).map_err(|error| {
        match error {
            wasmer::InstantiationError::Link(wasmer::LinkError::Resource(e))
                if e.contains("memory is greater than the maximum allowed memory") =>
            {
                // TODO: This is not good!, if the error message changes, our code will break,
                //       but for now, we do not have any other way to get the actual error case.
                //       Maybe create a `MemoryError::generic(String)` and use a constant identifier in
                //       it?

                Error::FunctionRuntimeError(FunctionRuntimeError::MaximumMemoryExceeded)
            }
            e => Error::FunctionLoadingError(FunctionLoadingError::FailedToInstantiateWasmModule(
                Box::new(e),
            )),
        }
    })?;

    // If this module exports an _initialize function, run that first.
    let join_handle = tokio::task::spawn_blocking(move || {
        if let Ok(initialize) = instance.exports.get_function("_initialize") {
            initialize.call(&mut store, &[]).map_err(|e| {
                (
                    Error::FunctionRuntimeError(
                        FunctionRuntimeError::FunctionInitializationFailed(e),
                    ),
                    points_to_instruction_count(
                        get_remaining_points(&mut store, &instance),
                        giga_instructions_limit,
                    ),
                )
            })?;
        }

        let start = instance.exports.get_function("_start").map_err(|e| {
            (
                Error::FunctionRuntimeError(FunctionRuntimeError::MissingStartFunction(e)),
                points_to_instruction_count(
                    get_remaining_points(&mut store, &instance),
                    giga_instructions_limit,
                ),
            )
        })?;

        let result = start
            .call(&mut store, &[])
            .map(|_| get_remaining_points(&mut store, &instance))
            .map_err(|e| (e, get_remaining_points(&mut store, &instance)));

        match (result, giga_instructions_limit) {
            (Ok(points), limit) => Ok(points_to_instruction_count(points, limit)),

            (Err((_, MeteringPoints::Exhausted)), limit) => Err((
                Error::Timeout,
                points_to_instruction_count(MeteringPoints::Exhausted, limit),
            )),

            (Err((_, MeteringPoints::Remaining(points))), limit) => Err((
                Error::FunctionDidntTerminateCleanly,
                points_to_instruction_count(MeteringPoints::Remaining(points), limit),
            )),
        }
    });

    Ok(FunctionHandle::new(
        join_handle,
        FunctionIO {
            stdin: stdin_tx,
            stdout: stdout_rx,
            stderr: stderr_rx,
        },
    ))
}

#[inline]
fn points_to_instruction_count(
    points: MeteringPoints,
    giga_instructions_limit: Option<u32>,
) -> u64 {
    let limit = giga_instructions_limit.map(|p| p as u64 * 1_000_000_000u64);
    match (points, limit) {
        (MeteringPoints::Remaining(r), None) => u64::MAX - r,
        (MeteringPoints::Remaining(r), Some(limit)) => limit - r,
        (MeteringPoints::Exhausted, None) => u64::MAX,
        (MeteringPoints::Exhausted, Some(limit)) => limit,
    }
}
