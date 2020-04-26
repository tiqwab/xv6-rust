use crate::file;
use crate::file::{File, FileTableEntry};
use crate::rwlock::RwLock;
use alloc::sync::Arc;
use consts::*;

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
}

/// Return (file for read, file for write) if successful.
pub(crate) fn alloc() -> Option<(FileTableEntry, FileTableEntry)> {
    let mut ft = file::file_table();
    let p = Arc::new(RwLock::new(Pipe::new()));
    ft.alloc_as_pipe(&p)
}

pub(crate) fn close(_p: Arc<RwLock<Pipe>>, _writable: bool) {
    // maybe nothing to do
    // released automatically
}
