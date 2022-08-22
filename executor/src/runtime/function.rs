//TODO
#![allow(dead_code)]

use std::{
    collections::HashMap,
    hash::Hash,
    io::{BufReader, BufWriter},
};

use anyhow::Result;
use tokio::task::JoinHandle;
use wasmer::{Instance, Module, Store};
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
        let envs = envs.into_iter().collect();
        Self {
            id,
            source,
            runtime,
            envs,
        }
    }
}

pub struct FunctionIO {
    pub stdin: BufWriter<Pipe>,
    pub stdout: BufReader<Pipe>,
    pub stderr: BufReader<Pipe>,
}

pub struct FunctionHandle {
    pub join_handle: JoinHandle<()>,
    pub io: FunctionIO,
}

//TODO: configure `Builder` of tokio for huge blocking tasks
pub fn start(definition: &FunctionDefinition) -> Result<FunctionHandle> {
    let mut store = Store::default();
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

    let join_handle = tokio::task::spawn_blocking(move || {
        //TODO: bubble up the error to outer task
        let start = instance.exports.get_function("_start").unwrap();
        start.call(&mut store, &[]).unwrap();
        //TODO: report usage to runtime
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
