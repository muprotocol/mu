use serde::{Deserialize, Serialize};
use std::any::type_name;
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::io::{BufReader, BufWriter};
use std::marker::PhantomData;
use wasmer_wasi::Pipe;

use super::function::FunctionPipes;

#[derive(Deserialize, Serialize)]
pub struct ID<T> {
    inner: [u8; 32],
    #[serde(skip)]
    phantom_type: PhantomData<T>,
}

impl<T> ID<T> {
    pub fn new(id: [u8; 32]) -> Self {
        Self {
            inner: id,
            phantom_type: PhantomData,
        }
    }

    pub fn gen() -> Self {
        Self::new(rand::random())
    }
}

impl<T> Hash for ID<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.hash(state)
    }
}

impl<T> PartialEq for ID<T> {
    fn eq(&self, other: &Self) -> bool {
        self.phantom_type == other.phantom_type && self.inner == other.inner
    }
}

impl<T> Eq for ID<T> {}

impl<T> Clone for ID<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner,
            phantom_type: PhantomData,
        }
    }
}

impl<T> Copy for ID<T> {}

impl<T> Debug for ID<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[")?;
        f.write_str(type_name::<T>())?;
        f.write_str("] ")?;
        self.inner.fmt(f)
    }
}

impl<T> Display for ID<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in self.inner {
            std::fmt::LowerHex::fmt(&b, f)?;
        }
        Ok(())
    }
}

pub struct FunctionIO {
    pub stdin: BufWriter<Pipe>,
    pub stdout: BufReader<Pipe>,
    pub stderr: BufReader<Pipe>,
}

impl FunctionIO {
    pub fn from_pipes(pipes: FunctionPipes) -> Self {
        Self {
            stdin: BufWriter::new(pipes.stdin),
            stdout: BufReader::new(pipes.stdout),
            stderr: BufReader::new(pipes.stderr),
        }
    }
}
