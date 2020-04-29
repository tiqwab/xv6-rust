use crate::constants::*;
use crate::fs::Inode;
use crate::pipe::Pipe;
use crate::rwlock::RwLock;
use crate::spinlock::{Mutex, MutexGuard};
use crate::{fs, log, pipe};
use alloc::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq)]
enum FileType {
    Pipe,
    Inode,
}

// FIXME: File should be enum consisting of Pipe and Inode
pub(crate) struct File {
    typ: FileType,
    readable: bool,
    writable: bool,
    pipe: Option<Arc<RwLock<Pipe>>>,
    ip: Option<Arc<RwLock<Inode>>>,
    off: u32,
}

impl File {
    fn new_for_inode(readable: bool, writable: bool, ip: &Arc<RwLock<Inode>>) -> File {
        File {
            typ: FileType::Inode,
            readable,
            writable,
            pipe: None,
            ip: Some(Arc::clone(ip)),
            off: 0,
        }
    }

    fn new_for_pipe(readable: bool, writable: bool, p: &Arc<RwLock<Pipe>>) -> File {
        File {
            typ: FileType::Pipe,
            readable,
            writable,
            pipe: Some(Arc::clone(p)),
            ip: None,
            off: 0,
        }
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
    pub(crate) fn read(&mut self, addr: *mut u8, n: usize) -> Result<usize, SysError> {
        if !self.readable {
            return Err(SysError::IllegalFileDescriptor);
        }

        match self.typ {
            FileType::Pipe => {
                let mut p = self.pipe.as_mut().expect("pipe should exist").write();
                p.read(addr, n)
            }
            FileType::Inode => {
                let ip = self.ip.as_ref().unwrap();
                let mut inode = fs::ilock(&ip);
                let cnt_opt = fs::readi(&mut inode, addr, self.off, n as u32);
                let res = match cnt_opt {
                    None => Err(SysError::TryAgain),
                    Some(cnt) => {
                        if cnt > 0 {
                            self.off += cnt;
                        }
                        Ok(cnt as usize)
                    }
                };
                fs::iunlock(inode);
                res
            }
        }
    }

    /// Write to file.
    pub(crate) fn write(&mut self, addr: *const u8, n: usize) -> Result<usize, SysError> {
        if !self.writable {
            return Err(SysError::IllegalFileDescriptor);
        }

        match self.typ {
            FileType::Pipe => {
                let mut p = self.pipe.as_mut().expect("pipe should exist").write();
                p.write(addr, n)
            }
            FileType::Inode => {
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

                Ok(n)
            }
        }
    }
}

pub(crate) struct FileTable {
    files: [Option<Arc<RwLock<File>>>; NFILE],
}

#[derive(Clone)]
pub(crate) struct FileTableEntry {
    pub(crate) file: Arc<RwLock<File>>,
    pub(crate) index: usize,
}

impl FileTable {
    const fn new() -> FileTable {
        FileTable {
            files: [None; NFILE],
        }
    }

    fn find_empty_entry(&self) -> Option<usize> {
        for (i, f_opt) in self.files.iter().enumerate() {
            if f_opt.is_none() {
                return Some(i);
            }
        }
        None
    }

    /// Allocate a file structure for inode.
    pub(crate) fn alloc_as_inode(
        &mut self,
        readable: bool,
        writable: bool,
        ip: &Arc<RwLock<Inode>>,
    ) -> Option<FileTableEntry> {
        match self.find_empty_entry() {
            None => None,
            Some(i) => {
                let f = Arc::new(RwLock::new(File::new_for_inode(readable, writable, ip)));
                self.files[i] = Some(Arc::clone(&f));
                Some(FileTableEntry { file: f, index: i })
            }
        }
    }

    /// Allocate a file structure for pipe
    /// Return (file for read, file for write) if successful.
    pub(crate) fn alloc_as_pipe(
        &mut self,
        p: &Arc<RwLock<Pipe>>,
    ) -> Option<(FileTableEntry, FileTableEntry)> {
        fn alloc(
            ft: &mut FileTable,
            i: usize,
            readable: bool,
            writable: bool,
            p: &Arc<RwLock<Pipe>>,
        ) -> FileTableEntry {
            let f = Arc::new(RwLock::new(File::new_for_pipe(readable, writable, p)));
            ft.files[i] = Some(Arc::clone(&f));
            FileTableEntry { file: f, index: i }
        }

        let ent0 = match self.find_empty_entry() {
            None => return None,
            Some(i) => alloc(self, i, true, false, p),
        };
        let ent1 = match self.find_empty_entry() {
            None => {
                self.close(ent0);
                return None;
            }
            Some(i) => alloc(self, i, false, true, p),
        };
        Some((ent0, ent1))
    }

    /// Close file f. (Decrement ref count, close when reaches 0.)
    pub(crate) fn close(&mut self, entry: FileTableEntry) {
        let ref_cnt = Arc::strong_count(&entry.file);

        if ref_cnt <= 1 {
            panic!("FileTable::close: illegal ref_cnt");
        } else if ref_cnt == 2 {
            // it means only me refers to the file because FileTable itself has one reference.
            let ind = entry.index;
            let mut f = entry.file.write();
            let typ = f.typ;

            if typ == FileType::Pipe {
                pipe::close(f.pipe.take().unwrap(), f.writable);
            } else if typ == FileType::Inode {
                if let Some(orig_ip) = &f.ip {
                    // FIXME: how to handle ownership of ip correctly...
                    let ip = Arc::clone(orig_ip);
                    // drop(entry);

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

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct FileDescriptor(pub(crate) u32);
