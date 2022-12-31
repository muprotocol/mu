//TODO
#![allow(dead_code)]

use std::{
    collections::HashMap,
    io::{BufReader, BufWriter},
};

use super::{
    error::{Error, FunctionLoadingError, FunctionRuntimeError},
    types::{FunctionHandle, FunctionIO},
};
use anyhow::Result;
use wasmer::{Instance, Module, Store};
use wasmer_middlewares::metering::get_remaining_points;
use wasmer_wasi::{Pipe, WasiState};

//TODO: configure `Builder` of tokio for huge blocking tasks
pub fn start(
    mut store: Store,
    module: &Module,
    envs: HashMap<String, String>,
) -> Result<FunctionHandle, Error> {
    //TODO: Check wasi version specified in this module and if we can run it!

    let stdin = Pipe::new();
    let stdout = Pipe::new();
    let stderr = Pipe::new();

    let program_name = module.name().unwrap_or("module");
    let wasi_env = WasiState::new(program_name)
        .stdin(Box::new(stdin.clone()))
        .stdout(Box::new(stdout.clone()))
        .stderr(Box::new(stderr.clone()))
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
            e => {
                Error::FunctionLoadingError(FunctionLoadingError::FailedToInstantiateWasmModule(e))
            }
        }
    })?;

    let memory = instance
        .exports
        .get_memory("memory")
        .map_err(|e| Error::FunctionLoadingError(FunctionLoadingError::FailedToGetMemory(e)))?;

    wasi_env.data_mut(&mut store).set_memory(memory.clone());

    let (is_finished_tx, is_finished_rx) = tokio::sync::oneshot::channel::<()>();

    // If this module exports an _initialize function, run that first.
    let join_handle = tokio::task::spawn_blocking(move || {
        if let Ok(initialize) = instance.exports.get_function("_initialize") {
            initialize.call(&mut store, &[]).map_err(|e| {
                (
                    Error::FunctionRuntimeError(
                        FunctionRuntimeError::FunctionInitializationFailed(e),
                    ),
                    get_remaining_points(&mut store, &instance),
                )
            })?;
        }

        let start = instance.exports.get_function("_start").map_err(|e| {
            (
                Error::FunctionRuntimeError(FunctionRuntimeError::MissingStartFunction(e)),
                get_remaining_points(&mut store, &instance),
            )
        })?;

        start.call(&mut store, &[]).map_err(|e| {
            (
                Error::FunctionRuntimeError(FunctionRuntimeError::FunctionEarlyExit(e)),
                get_remaining_points(&mut store, &instance),
            )
        })?;

        if let Err(e) = is_finished_tx.send(()) {
            log::error!("error sending finish signal: {e:?}");
        }

        Ok(get_remaining_points(&mut store, &instance))
    });

    Ok(FunctionHandle::new(
        join_handle,
        is_finished_rx,
        FunctionIO {
            stdin: BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
            stderr: BufReader::new(stderr),
        },
    ))
}
