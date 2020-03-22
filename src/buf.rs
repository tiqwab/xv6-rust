use crate::constants::*;
use crate::ide;
use crate::spinlock::Mutex;
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
    pub(crate) prev: *mut Buf, // LRU cache list
    pub(crate) next: *mut Buf,
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
            prev: null_mut(),
            next: null_mut(),
            qnext: null_mut(),
            data: [0; BLK_SIZE],
        }
    }
}

/// Buffer cache.
///
/// The buffer cache is a linked list of buf structures holding
/// cached copies of disk block contents.  Caching disk blocks
/// in memory reduces the number of disk reads and also provides
/// a synchronization point for disk blocks used by multiple processes.
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
    buf: [Buf; NBUF],
    // Linked list of all buffers, through prev/next.
    // head.next is most recently used.
    head: Buf,
}

unsafe impl Send for BufCache {}
unsafe impl Sync for BufCache {}

impl BufCache {
    const fn new() -> BufCache {
        BufCache {
            buf: [Buf::new(); NBUF],
            head: Buf::new(),
        }
    }

    fn get(&mut self, dev: u32, blockno: u32) -> &mut Buf {
        // Is the block already cached?
        let ptr = self.head.prev;
        while ptr != &mut self.head {
            unsafe {
                let b = &mut *ptr;
                if b.dev == dev && b.blockno == blockno {
                    return b;
                }
                ptr.add(1);
            }
        }

        // Not cached; recycle an unused buffer.
        // Even if refcnt==0, B_DIRTY indicates a buffer is in use
        // because log.c has modified it but not yet committed it.
        let ptr = self.head.prev;
        while ptr != &mut self.head {
            unsafe {
                let b = &mut *ptr;
                if b.refcnt == 0 && (b.flags & BUF_FLAGS_DIRTY) == 0 {
                    b.dev = dev;
                    b.blockno = blockno;
                    b.flags = 0;
                    b.refcnt = 1;
                    return b;
                }
            }
        }

        panic!("bget: no buffers");
    }

    /// Return a buf with the contents of the indicated block.
    pub(crate) fn read(&mut self, dev: u32, blockno: u32) -> &mut Buf {
        let b = self.get(dev, blockno);
        if b.flags & BUF_FLAGS_VALID == 0 {
            ide::ide_rw(b);
        }
        b
    }

    /// Write b's contents to disk.
    pub(crate) fn write(&mut self, b: &mut Buf) {
        b.flags |= BUF_FLAGS_DIRTY;
        ide::ide_rw(b);
    }

    /// Release a buffer.
    /// Move to the head of the MRU list.
    pub(crate) fn release(&mut self, b: &mut Buf) {
        b.refcnt -= 1;
        if b.refcnt == 0 {
            // no one is waiting for it
            unsafe {
                (*b.next).prev = b.prev;
                (*b.prev).next = b.next;
                b.next = self.head.next;
                b.prev = &mut self.head;
                (*self.head.next).prev = b;
                self.head.next = b;
            }
        }
    }
}

static BUF_CACHE: Mutex<BufCache> = Mutex::new(BufCache::new());

pub(crate) fn buf_init() {
    let mut cache = BUF_CACHE.lock();

    // Create linked list of buffers
    //       -next->     -next->        -next->
    // head           bn         ... b1         head
    //       <-prev-     <-prev-        <-prev-

    cache.head.prev = &mut cache.head;
    cache.head.next = &mut cache.head;

    let head_ptr = &mut cache.head as *mut Buf;
    for b in cache.buf.iter_mut() {
        unsafe {
            b.next = (*head_ptr).next;
            b.prev = head_ptr;
            (*(*head_ptr).next).prev = b;
            (*head_ptr).next = b;
        }
    }

    // for write test
    // let b = cache.get(1, 1);
    // let str = "foobar";
    // let src = crate::pmap::VirtAddr(str.as_ptr() as u32);
    // let dst = crate::pmap::VirtAddr(b.data.as_ptr() as u32);
    // unsafe { crate::util::memcpy(dst, src, str.len()) };
    // cache.write(b);
    // cache.release(b);

    // for read test
    // check buf.data at the last of ide_intr
    // let mut b = Buf::new();
    // b.dev = 1;
    // b.blockno = 1;
    // println!("buf.data: {:p}", &b.data);
    // ide_rw(&mut b);
}
