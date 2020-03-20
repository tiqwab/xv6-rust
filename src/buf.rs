use crate::constants::*;
use core::ptr::null_mut;

pub(crate) mod consts {
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
    pub(crate) prev: *mut Buf, // LRU cache list
    pub(crate) next: *mut Buf,
    pub(crate) qnext: *mut Buf, // disk queue
    pub(crate) data: [u8; BLK_SIZE],
}

impl Buf {
    pub(crate) fn new() -> Buf {
        Buf {
            flags: 0,
            dev: 0,
            blockno: 0,
            refcnt: 0,
            prev: null_mut(),
            next: null_mut(),
            qnext: null_mut(),
            data: [0; BLK_SIZE],
        }
    }
}
