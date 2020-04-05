use crate::constants::*;
use crate::fs::Inode;
use crate::rwlock::RwLock;
use crate::spinlock::{Mutex, MutexGuard};
use crate::{fs, log};
use alloc::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq)]
enum FileType {
    None,
    Pipe,
    Inode,
}

pub(crate) struct File {
    typ: FileType,
    readable: bool,
    writable: bool,
    // pipe: Pipe,
    ip: Option<Arc<RwLock<Inode>>>,
    off: u32,
}

impl File {
    fn new() -> File {
        File {
            typ: FileType::None,
            readable: false,
            writable: false,
            ip: None,
            off: 0,
        }
    }

    pub(crate) fn init_as_inode(
        &mut self,
        readable: bool,
        writable: bool,
        ip: &Arc<RwLock<Inode>>,
    ) {
        self.typ = FileType::Inode;
        self.ip = Some(Arc::clone(ip));
        self.readable = readable;
        self.writable = writable;
        self.off = 0;
    }

    pub(crate) fn stat(&self) -> Option<fs::Stat> {
        if self.typ == FileType::Inode {
            let ip = self.ip.as_ref().unwrap();
            let mut inode = fs::ilock(&ip);
            let stat = fs::stati(&mut inode);
            fs::iunlock(inode);
            Some(stat)
        } else {
            None
        }
    }

    /// Read from file.
    pub(crate) fn read(&mut self, addr: *mut u8, n: usize) -> Option<usize> {
        if !self.readable {
            return None;
        }

        if self.typ == FileType::Pipe {
            unimplemented!()
        }

        if self.typ != FileType::Inode {
            panic!("File::read: unexpected type");
        }

        let ip = self.ip.as_ref().unwrap();
        let mut inode = fs::ilock(&ip);
        let r = fs::readi(&mut inode, addr, self.off, n as u32);
        if r > 0 {
            self.off += r;
        }
        fs::iunlock(inode);
        Some(r as usize)
    }

    /// Write to file.
    pub(crate) fn write(&mut self, addr: *const u8, n: usize) -> Option<usize> {
        if !self.writable {
            return None;
        }

        if self.typ == FileType::Pipe {
            unimplemented!()
        }

        if self.typ != FileType::Inode {
            panic!("File::write: unexpected type");
        }

        // write a few blocks at a time to avoid exceeding
        // the maximum log transaction size, including
        // i-node, indirect block, allocation blocks,
        // and 2 blocks of slop for non-aligned writes.
        // this really belongs lower down, since writei()
        // might be writing a device like the console.
        let max = ((MAX_OP_BLOCKS - 1 - 1 - 2) / 2) * 512;
        let mut i = 0;
        while i < n {
            let mut n1 = n - i;
            if n1 > max {
                n1 = max;
            }

            log::begin_op();
            let ip = self.ip.as_ref().unwrap();
            let mut inode = fs::ilock(&ip);
            let r = fs::writei(&mut inode, addr, self.off, n as u32);
            if r > 0 {
                self.off += r;
            }
            fs::iunlock(inode);
            log::end_op();

            if r != n1 as u32 {
                panic!("File::write: short file write");
            }

            i += r as usize;
        }

        Some(n)
    }
}

pub(crate) struct FileTable {
    files: [Option<Arc<File>>; NFILE], // FIXME: should be Arc<RwLock<File>>?
}

pub(crate) struct FileTableEntry {
    pub(crate) file: Arc<File>,
    pub(crate) index: usize,
}

impl FileTable {
    const fn new() -> FileTable {
        FileTable {
            files: [None; NFILE],
        }
    }

    /// Allocate a file structure.
    pub(crate) fn alloc_as_inode(
        &mut self,
        readable: bool,
        writable: bool,
        ip: &Arc<RwLock<Inode>>,
    ) -> Option<FileTableEntry> {
        for (i, f_opt) in self.files.iter_mut().enumerate() {
            if f_opt.is_none() {
                let mut f = File::new();
                f.init_as_inode(readable, writable, ip);
                let f = Arc::new(f);
                *f_opt = Some(Arc::clone(&f));
                return Some(FileTableEntry { file: f, index: i });
            }
        }
        None
    }

    /// Increment ref count for file f.
    pub(crate) fn dup(&self, entry: &mut Arc<FileTableEntry>) -> FileTableEntry {
        FileTableEntry {
            file: Arc::clone(&entry.file),
            index: entry.index,
        }
    }

    /// Close file f. (Decrement ref count, close when reaches 0.)
    pub(crate) fn close(&mut self, entry: FileTableEntry) {
        let ref_cnt = Arc::strong_count(&entry.file);

        if ref_cnt <= 1 {
            panic!("FileTable::close: illegal ref_cnt");
        } else if ref_cnt == 2 {
            // it means only me refers to the file because FileTable itself has one reference.
            let ind = entry.index;
            let typ = entry.file.typ;

            if typ == FileType::Pipe {
                unimplemented!()
            } else if typ == FileType::Inode {
                if let Some(orig_ip) = &entry.file.ip {
                    // FIXME: how to handle ownership of ip correctly...
                    let ip = Arc::clone(orig_ip);
                    drop(entry);

                    log::begin_op();
                    fs::iput(ip);
                    log::end_op();
                }
            }

            self.files[ind] = None;
        } else {
            // just drop file
        }
    }
}

static FILE_TABLE: Mutex<FileTable> = Mutex::new(FileTable::new());

pub(crate) fn file_table() -> MutexGuard<'static, FileTable> {
    FILE_TABLE.lock()
}

pub(crate) struct FileDescriptor(pub(crate) u32);
