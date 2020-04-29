use crate::constants::SysError;
use crate::file;
use crate::file::{File, FileTableEntry};
use crate::rwlock::RwLock;
use alloc::sync::Arc;
use consts::*;
use core::cmp;

pub(crate) mod consts {
    pub(crate) const PIPE_SIZE: usize = 512;
}

pub(crate) struct Pipe {
    data: [u8; PIPE_SIZE],
    nread: u32,       // number of bytes read
    nwrite: u32,      // number of bytes written
    read_open: bool,  // read fd is still open
    write_open: bool, // write fd is still open
}

impl Pipe {
    fn new() -> Pipe {
        Pipe {
            data: [0; PIPE_SIZE],
            nread: 0,
            nwrite: 0,
            read_open: true,
            write_open: true,
        }
    }

    /// Read from pipe.
    ///
    /// Return read bytes if successful.
    /// Return 0 if there is no data and write-edge of the pipe is already closed,
    /// otherwise return SysError::TryAgain (and caller will retry again).
    pub(crate) fn read(&mut self, addr: *mut u8, n: usize) -> Result<usize, SysError> {
        let mut len = cmp::min((self.nwrite - self.nread) as usize, n);
        if len == 0 {
            if !self.write_open {
                return Ok(0);
            } else {
                return Err(SysError::TryAgain);
            }
        }

        for i in 0..len {
            let c = self.data[self.nread as usize % PIPE_SIZE];
            unsafe { *addr.add(i) = c };
            self.nread += 1;
        }

        Ok(len)
    }

    /// Write from pipe.
    ///
    /// Return written bytes if successful.
    /// Return SysError::BrokenPipe if read-edge of the pipe is already closed.
    /// Return SysError::TryAgain if the pipe doesn't have enough buffer (and caller will retry again).
    pub(crate) fn write(&mut self, addr: *const u8, n: usize) -> Result<usize, SysError> {
        if !self.read_open {
            return Err(SysError::BrokenPipe);
        }

        if self.nwrite + (n as u32) > self.nread + (PIPE_SIZE as u32) {
            return Err(SysError::TryAgain);
        }

        for i in 0..n {
            let c = unsafe { *addr.add(i) };
            self.data[self.nwrite as usize % PIPE_SIZE] = c;
            self.nwrite += 1;
        }
        Ok(n)
    }
}

/// Return (file for read, file for write) if successful.
pub(crate) fn alloc() -> Option<(FileTableEntry, FileTableEntry)> {
    let mut ft = file::file_table();
    let p = Arc::new(RwLock::new(Pipe::new()));
    ft.alloc_as_pipe(&p)
}

pub(crate) fn close(pipe: Arc<RwLock<Pipe>>, writable: bool) {
    let mut p = pipe.write();
    if writable {
        p.write_open = false;
    } else {
        p.read_open = false;
    }
}
