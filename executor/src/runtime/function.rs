//TODO
#![allow(dead_code)]

use std::{
    collections::HashMap,
    hash::Hash,
    io::{BufReader, BufWriter},
    sync::Arc,
};

use anyhow::{Context, Result};
use tokio::task::JoinHandle;
use wasmer::{CompilerConfig, Cranelift, EngineBuilder, Instance, Module, Store};
use wasmer_middlewares::{
    metering::{get_remaining_points, MeteringPoints},
    Metering,
};
use wasmer_wasi::{Pipe, WasiState};

use crate::mu_stack::{FunctionRuntime, StackID};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct FunctionID {
    pub stack_id: StackID,
    pub function_name: String,
}

pub type FunctionSource = Vec<u8>;

#[derive(Clone, Debug)]
pub struct FunctionDefinition {
    pub id: FunctionID,
    pub source: FunctionSource,
    pub runtime: FunctionRuntime,

    // TODO: key must not contain `=` and both must not contain `null` byte
    pub envs: HashMap<String, String>,
}

impl FunctionDefinition {
    pub fn new(
        id: FunctionID,
        source: FunctionSource,
        runtime: FunctionRuntime,
        envs: impl IntoIterator<
            IntoIter = impl Iterator<Item = (String, String)>,
            Item = (String, String),
        >,
    ) -> Self {
        let mut envs: HashMap<String, String> = envs.into_iter().collect();
        envs.insert("RUST_BACKTRACE".into(), "1".into()); //TODO: remove this
        Self {
            id,
            source,
            runtime,
            envs,
        }
    }
}

#[derive(Debug)]
pub struct FunctionIO {
    pub stdin: BufWriter<Pipe>,
    pub stdout: BufReader<Pipe>,
    pub stderr: BufReader<Pipe>,
}

#[derive(Debug)]
pub struct FunctionHandle {
    pub join_handle: Arc<JoinHandle<MeteringPoints>>,
    pub io: FunctionIO,
}

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
        join_handle: Arc::new(join_handle),
        io: FunctionIO {
            stdin: BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
            stderr: BufReader::new(stderr),
        },
    })
}
