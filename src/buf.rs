use crate::constants::*;
use crate::ide;
use crate::spinlock::{Mutex, MutexGuard};
use consts::*;
use core::ptr::null_mut;

pub(crate) mod consts {
    use crate::constants::MAX_OP_BLOCKS;

    pub(crate) const NBUF: usize = MAX_OP_BLOCKS * 3;

    // flags
    pub(crate) const BUF_FLAGS_VALID: u32 = 0x2; // buffer has been read from disk
    pub(crate) const BUF_FLAGS_DIRTY: u32 = 0x4; // buffer needs to be written to disk
}

pub(crate) struct Buf {
    pub(crate) flags: u32,
    pub(crate) dev: u32,
    pub(crate) blockno: u32,
    // lock: SleepLock,
    pub(crate) refcnt: u32,
    pub(crate) qnext: *mut Buf, // disk queue
    pub(crate) data: [u8; BLK_SIZE],
}

impl Buf {
    pub(crate) const fn new() -> Buf {
        Buf {
            flags: 0,
            dev: 0,
            blockno: 0,
            refcnt: 0,
            qnext: null_mut(),
            data: [0; BLK_SIZE],
        }
    }
}

struct BufCacheHandler<'a> {
    buf: &'a mut Buf,
}

impl<'a> BufCacheHandler<'a> {
    fn read(&mut self) {
        if self.buf.flags & BUF_FLAGS_VALID == 0 {
            ide::ide_rw(self.buf);
        }
    }

    fn write(&mut self) {
        self.buf.flags |= BUF_FLAGS_DIRTY;
        ide::ide_rw(self.buf);
    }
}

/// Buffer cache.
///
/// The buffer cache holds cached copies of disk block contents.
/// Caching disk blocks in memory reduces the number of disk reads
/// and also provides a synchronization point for disk blocks used
/// by multiple processes.
///
/// Interface:
/// * To get a buffer for a particular disk block, call bread.
/// * After changing buffer data, call bwrite to write it to disk.
/// * When done with the buffer, call brelse.
/// * Do not use the buffer after calling brelse.
/// * Only one process at a time can use a buffer,
///     so do not keep them longer than necessary.
///
/// The implementation uses two state flags internally:
/// * B_VALID: the buffer data has been read from the disk.
/// * B_DIRTY: the buffer data has been modified and needs to be written to disk.
struct BufCache {
    entries: [Option<Buf>; NBUF],
}

unsafe impl Send for BufCache {}
unsafe impl Sync for BufCache {}

impl BufCache {
    const fn new() -> BufCache {
        BufCache {
            entries: [None; NBUF],
        }
    }

    fn get(&mut self, dev: u32, blockno: u32) -> BufCacheHandler {
        let mut empty_entry = None;

        // Is the block already cached?
        for entry_opt in self.entries.iter_mut() {
            match entry_opt {
                None => {
                    empty_entry = Some(entry_opt);
                }
                Some(buf) => {
                    if buf.dev == dev && buf.blockno == blockno {
                        buf.refcnt += 1;
                        return BufCacheHandler { buf };
                    }
                }
            }
        }

        // Not cached; recycle an unused buffer.
        // Even if refcnt==0, B_DIRTY indicates a buffer is in use
        // because log.c has modified it but not yet committed it.
        match empty_entry {
            None => {
                panic!("get: no buffers");
            }
            Some(entry_ref) => {
                let mut buf = Buf::new();
                buf.dev = dev;
                buf.blockno = blockno;
                buf.flags = 0;
                buf.refcnt = 1;
                *entry_ref = Some(buf);

                BufCacheHandler {
                    buf: entry_ref.as_mut().unwrap(),
                }
            }
        }
    }

    fn release(&mut self, dev: u32, blockno: u32) {
        for entry_opt in self.entries.iter_mut() {
            match entry_opt {
                None => {}
                Some(buf) => {
                    if buf.dev == dev && buf.blockno == blockno {
                        buf.refcnt -= 1;
                        if buf.refcnt == 0 {
                            *entry_opt = None;
                        }
                        return;
                    }
                }
            }
        }

        panic!("release: illegal dev or blockno");
    }
}

static BUF_CACHE: Mutex<BufCache> = Mutex::new(BufCache::new());

pub(crate) fn buf_init() {
    {
        // for write test
        // let mut cache = BUF_CACHE.lock();
        // let mut b = cache.get(1, 1);
        // let str = "foobar";
        // let src = crate::pmap::VirtAddr(str.as_ptr() as u32);
        // let dst = crate::pmap::VirtAddr(b.buf.data.as_ptr() as u32);
        // unsafe { crate::util::memcpy(dst, src, str.len()) };
        // b.write();
        // cache.release(1, 1);
    }

    {
        // for read test
        // let mut cache = BUF_CACHE.lock();
        // let mut b = cache.get(1, 1);
        // b.read();
        // cache.release(1, 1);
    }
}
