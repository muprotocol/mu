use bytes::Buf;
use std::{
    collections::VecDeque,
    io::{self, Read, Seek, Write},
    sync::{Arc, Condvar, Mutex},
};
use wasmer_wasi::{FsError, VirtualFile};

// Re-implementation of wasmer's pipes with an optional Condvar for reading input
#[derive(Debug, Clone, Default)]
pub struct Pipe {
    arc: Arc<PipeInner>,
}

#[derive(Debug, Default)]
struct PipeInner {
    mutex: Mutex<PipeBuffer>,
    condvar: Condvar,
}

#[derive(Debug, Default)]
struct PipeBuffer {
    buffer: VecDeque<u8>,
    is_closed: bool,
}

impl Pipe {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn close(&mut self) {
        let mut guard = self.arc.mutex.lock().unwrap();
        guard.is_closed = true;
        self.arc.condvar.notify_all();
        drop(guard);
    }
}

impl Read for Pipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut guard = self.arc.mutex.lock().unwrap();
        if guard.buffer.is_empty() && !guard.is_closed {
            guard = self.arc.condvar.wait(guard).unwrap();
        }
        let amt = std::cmp::min(buf.len(), guard.buffer.len());
        guard.buffer.copy_to_slice(&mut buf[0..amt]);
        Ok(amt)
    }
}

impl Write for Pipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let mut guard = self.arc.mutex.lock().unwrap();
        guard.buffer.extend(buf);
        self.arc.condvar.notify_one();
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
        let guard = self.arc.mutex.lock().unwrap();
        Ok(Some(guard.buffer.len()))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        sync::{Arc, Mutex},
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

    #[test]
    fn close_while_waiting_to_read() {
        let mut pipe = Pipe::new();
        let mut pipe_clone = pipe.clone();
        let didnt_run_prematurely = Arc::new(Mutex::new(false));
        let didnt_run_prematurely_clone = didnt_run_prematurely.clone();

        let handle = thread::spawn(move || {
            let mut buf = [0u8; 5];
            assert_eq!(pipe.read(&mut buf).unwrap(), 0);
            assert!(*didnt_run_prematurely.lock().unwrap());
        });

        thread::sleep(Duration::from_millis(500));
        *didnt_run_prematurely_clone.lock().unwrap() = true;
        pipe_clone.close();

        handle.join().unwrap();
    }

    #[test]
    fn close_before_read() {
        let mut pipe = Pipe::new();
        let mut pipe_clone = pipe.clone();

        let handle = thread::spawn(move || {
            pipe_clone.close();
        });

        thread::sleep(Duration::from_millis(500));
        let mut buf = [0u8; 5];
        assert_eq!(pipe.read(&mut buf).unwrap(), 0);

        handle.join().unwrap();
    }

    #[test]
    fn close_before_read_on_same_thread() {
        let mut pipe = Pipe::new();

        pipe.close();

        let mut buf = [0u8; 5];
        assert_eq!(pipe.read(&mut buf).unwrap(), 0);
    }
}
