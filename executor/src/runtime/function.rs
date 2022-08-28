//TODO
#![allow(dead_code)]

use std::{
    io::{BufReader, BufWriter},
    sync::Arc,
};

use super::types::{FunctionDefinition, FunctionHandle, FunctionIO};
use anyhow::{Context, Result};
use wasmer::{CompilerConfig, Cranelift, EngineBuilder, Instance, Module, Store};
use wasmer_middlewares::{metering::get_remaining_points, Metering};
use wasmer_wasi::{Pipe, WasiState};

//TODO: configure `Builder` of tokio for huge blocking tasks
pub fn start(definition: &FunctionDefinition) -> Result<FunctionHandle> {
    //TODO: Check wasi version specified in this module and if we can run it!

    let mut compiler = Cranelift::default();
    let metering = Arc::new(Metering::new(u64::MAX, |_| 1));
    compiler.push_middleware(metering);

    let mut store = Store::new(EngineBuilder::new(compiler));

    //TODO: not good performance-wise to keep compiling the module
    let module = Module::from_binary(&store, &definition.source)?;

    let stdin = Pipe::new();
    let stdout = Pipe::new();
    let stderr = Pipe::new();

    let program_name = module.name().unwrap_or("module");
    let wasi_env = WasiState::new(program_name)
        .stdin(Box::new(stdin.clone()))
        .stdout(Box::new(stdout.clone()))
        .stderr(Box::new(stderr.clone()))
        .envs(definition.envs.clone())
        .finalize(&mut store)?;

    let import_object = wasi_env.import_object(&mut store, &module)?;
    let instance = Instance::new(&mut store, &module, &import_object)?;
    let memory = instance.exports.get_memory("memory")?;
    wasi_env.data_mut(&mut store).set_memory(memory.clone());

    // If this module exports an _initialize function, run that first.
    let join_handle = tokio::task::spawn_blocking(move || {
        //TODO: bubble up the error to outer task
        if let Ok(initialize) = instance.exports.get_function("_initialize") {
            initialize
                .call(&mut store, &[])
                .context("failed to run _initialize function")
                .unwrap();
        }

        let start = instance
            .exports
            .get_function("_start")
            .context("can not get _start function")
            .unwrap();

        let result = start.call(&mut store, &[]);

        match result {
            Ok(_) => (),
            Err(e) => println!("error: {e:?}"),
        };

        get_remaining_points(&mut store, &instance)
    });

    Ok(FunctionHandle {
        join_handle,
        io: FunctionIO {
            stdin: BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
            stderr: BufReader::new(stderr),
        },
    })
}
