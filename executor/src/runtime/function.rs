use std::{
    collections::{HashMap, VecDeque},
    io::{self, Read, Seek, Write},
    sync::{Arc, Condvar, Mutex},
};

use super::{
    error::{Error, FunctionLoadingError, FunctionRuntimeError},
    types::{FunctionHandle, FunctionIO},
};
use anyhow::Result;
use bytes::Buf;
use wasmer::{Instance, Module, Store};
use wasmer_middlewares::metering::get_remaining_points;
use wasmer_wasi::{FsError, VirtualFile, WasiState};

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
            stdin,
            stdout,
            stderr,
        },
    ))
}

// Re-implementation of wasmer's pipes with an optional Condvar for reading input
#[derive(Debug, Clone, Default)]
pub struct Pipe {
    buffer: Arc<(Mutex<VecDeque<u8>>, Condvar)>,
}

impl Pipe {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Read for Pipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut buffer = self.buffer.0.lock().unwrap();
        if buffer.is_empty() {
            buffer = self.buffer.1.wait(buffer).unwrap();
        }
        let amt = std::cmp::min(buf.len(), buffer.len());
        buffer.copy_to_slice(&mut buf[0..amt]);
        Ok(amt)
    }
}

impl Write for Pipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let mut buffer = self.buffer.0.lock().unwrap();
        buffer.extend(buf);
        self.buffer.1.notify_one();
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Seek for Pipe {
    fn seek(&mut self, _pos: io::SeekFrom) -> io::Result<u64> {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "can not seek in a pipe",
        ))
    }
}

impl VirtualFile for Pipe {
    fn last_accessed(&self) -> u64 {
        0
    }
    fn last_modified(&self) -> u64 {
        0
    }
    fn created_time(&self) -> u64 {
        0
    }
    fn size(&self) -> u64 {
        0
    }
    fn set_len(&mut self, _len: u64) -> Result<(), FsError> {
        Ok(())
    }
    fn unlink(&mut self) -> Result<(), FsError> {
        Ok(())
    }
    fn bytes_available_read(&self) -> Result<Option<usize>, FsError> {
        let buffer = self.buffer.0.lock().unwrap();
        Ok(Some(buffer.len()))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        thread,
        time::Duration,
    };

    use super::Pipe;

    #[test]
    fn read_then_write() {
        let mut pipe = Pipe::new();
        let mut pipe_clone = pipe.clone();

        let handle = thread::spawn(move || {
            let mut buf = [0u8; 5];
            pipe.read_exact(&mut buf).unwrap();
            assert_eq!(buf, [1, 2, 3, 4, 5]);
        });

        thread::sleep(Duration::from_millis(500));
        let buf = [1u8, 2, 3, 4, 5];
        assert_eq!(pipe_clone.write(&buf).unwrap(), 5);

        handle.join().unwrap();
    }

    #[test]
    fn write_then_read() {
        let mut pipe = Pipe::new();
        let mut pipe_clone = pipe.clone();

        let handle = thread::spawn(move || {
            let buf = [1u8, 2, 3, 4, 5];
            assert_eq!(pipe_clone.write(&buf).unwrap(), 5);
        });

        thread::sleep(Duration::from_millis(500));
        let mut buf = [0u8; 5];
        pipe.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [1, 2, 3, 4, 5]);

        handle.join().unwrap();
    }

    #[test]
    fn write_then_read_on_same_thread() {
        let mut pipe = Pipe::new();
        let mut pipe_clone = pipe.clone();

        let buf = [1u8, 2, 3, 4, 5];
        assert_eq!(pipe_clone.write(&buf).unwrap(), 5);

        let mut buf = [0u8; 5];
        pipe.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [1, 2, 3, 4, 5]);
    }

    #[test]
    fn partially_available_read() {
        let mut pipe = Pipe::new();
        let mut pipe_clone = pipe.clone();

        assert_eq!(pipe.write(&[1, 2]).unwrap(), 2);

        let handle = thread::spawn(move || {
            let mut buf = [0u8; 5];
            pipe.read_exact(&mut buf).unwrap();
            assert_eq!(buf, [1, 2, 3, 4, 5]);
        });

        thread::sleep(Duration::from_millis(500));
        assert_eq!(pipe_clone.write(&[3, 4, 5]).unwrap(), 3);

        handle.join().unwrap();
    }

    #[test]
    fn too_much_to_read() {
        let mut pipe = Pipe::new();
        let mut pipe_clone = pipe.clone();

        assert_eq!(pipe.write(&[1, 2]).unwrap(), 2);

        let handle = thread::spawn(move || {
            let mut buf = [0u8; 5];
            pipe.read_exact(&mut buf).unwrap();
            assert_eq!(buf, [1, 2, 3, 4, 5]);
            pipe.read_exact(&mut buf[0..3]).unwrap();
            assert_eq!(buf[0..3], [6, 7, 8]);
        });

        thread::sleep(Duration::from_millis(500));
        assert_eq!(pipe_clone.write(&[3, 4, 5, 6, 7, 8]).unwrap(), 6);

        handle.join().unwrap();
    }

    #[test]
    fn read_zero_bytes_without_writing() {
        let mut pipe = Pipe::new();
        let mut buf = [0u8; 0];
        pipe.read_exact(&mut buf).unwrap();
        assert_eq!(pipe.read(&mut buf).unwrap(), 0);
    }
}
