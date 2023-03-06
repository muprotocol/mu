use std::collections::HashMap;

use super::{
    error::{Error, FunctionLoadingError, FunctionRuntimeError, Result},
    pipe::Pipe,
    types::{FunctionHandle, FunctionIO},
};
use log::debug;
use wasmer::{Instance, Module, Store};
use wasmer_middlewares::metering::get_remaining_points;
use wasmer_wasi::WasiState;

//TODO: configure `Builder` of tokio for huge blocking tasks
pub fn start(
    mut store: Store,
    module: &Module,
    envs: HashMap<String, String>,
) -> Result<FunctionHandle> {
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
            e => Error::FunctionLoadingError(FunctionLoadingError::FailedToInstantiateWasmModule(
                Box::new(e),
            )),
        }
    })?;

    let memory = instance
        .exports
        .get_memory("memory")
        .map_err(|e| Error::FunctionLoadingError(FunctionLoadingError::FailedToGetMemory(e)))?;

    wasi_env.data_mut(&mut store).set_memory(memory.clone());

    let mut stdin_clone = stdin.clone();
    let mut stdout_clone = stdout.clone();
    let mut stderr_clone = stderr.clone();

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

        let result = start.call(&mut store, &[]);

        stdin_clone.close();
        stdout_clone.close();
        stderr_clone.close();

        result
            .map(|_| get_remaining_points(&mut store, &instance))
            .map_err(|e| {
                debug!("Function didn't terminated cleanly: {e:#?}");
                (
                    Error::FunctionDidntTerminateCleanly,
                    get_remaining_points(&mut store, &instance),
                )
            })
    });

    Ok(FunctionHandle::new(
        join_handle,
        FunctionIO {
            stdin,
            stdout,
            stderr,
        },
    ))
}
