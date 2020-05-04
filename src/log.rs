use crate::buf::BufCacheHandler;
use crate::constants::{BLK_SIZE, LOG_SIZE, MAX_OP_BLOCKS, ROOT_DEV};
use crate::once::Once;
use crate::pmap::VirtAddr;
use crate::spinlock::{Mutex, MutexGuard};
use crate::{buf, superblock, util};
use core::mem;

// Contents of the header block, used for both the on-disk header block
// and to keep track in memory of logged block# before commit.
//
// This is stored at the top of log blocks of disk
struct LogHeader {
    n: usize,
    block: [u32; LOG_SIZE],
}

impl LogHeader {
    /// Create a new empty LogHeader.
    /// It should be initialized with recover_from_log.
    const fn empty() -> LogHeader {
        LogHeader {
            n: 0,
            block: [0; LOG_SIZE],
        }
    }

    fn init(&mut self, lh: &LogHeader) {
        *self = LogHeader {
            n: lh.n,
            block: lh.block,
        }
    }
}

struct Log {
    start: usize,
    size: usize,
    outstanding: usize, // how many FS sys calls are executing
    // committing: bool,   // true if someone is in commit(). Please wait
    dev: u32,
    lh: LogHeader,
}

impl Log {
    /// Create a new Log.
    fn new(start: usize, size: usize, dev: u32) -> Log {
        Log {
            start,
            size,
            outstanding: 0,
            dev,
            lh: LogHeader::empty(),
        }
    }
}

static LOG: Once<Mutex<Log>> = Once::new();

fn get_log() -> MutexGuard<'static, Log> {
    LOG.call_once(|| Mutex::new(log_init(ROOT_DEV))).lock()
}

/// Called at the start of each FS system call.
pub(crate) fn begin_op() {
    // xv6 use sleep to wait, but use spin here for the simplicity.
    loop {
        let mut log = get_log();

        if log.lh.n + (log.outstanding + 1) * MAX_OP_BLOCKS > LOG_SIZE {
            // this op might exhaust log space; wait for commit
            continue;
        }

        log.outstanding += 1;
        break;
    }
}

/// Called at the end of each FS system call.
/// Commits if this was the last outstanding operation.
pub(crate) fn end_op() {
    let mut log = get_log();

    log.outstanding -= 1;

    if log.outstanding == 0 {
        // do commit
        commit(&mut log);
    }
}

fn commit(log: &mut Log) {
    if log.lh.n > 0 {
        write_log(log); // write modified blocks from cache to log
        write_head(log); // write header to disk -- the real commit
        install_trans(log); // now install writes to home locations
        log.lh.n = 0;
        write_head(log); // erase the transaction from the log
    }
}

/// Copy modified blocks from cache to log.
fn write_log(log: &Log) {
    let mut bcache = buf::buf_cache();

    for tail in 0..(log.lh.n) {
        let mut buf_to = bcache.get(log.dev, (log.start + tail + 1) as u32);
        buf_to.read();
        let mut buf_from = bcache.get(log.dev, log.lh.block[tail]);
        buf_from.read();

        unsafe {
            let dst = VirtAddr(buf_to.data().as_ptr() as u32);
            let src = VirtAddr(buf_from.data().as_ptr() as u32);
            let len = BLK_SIZE;
            util::memmove(dst, src, len);
        }

        buf_to.write();
        bcache.release(buf_from);
        bcache.release(buf_to);
    }
}

/// Write in-memory log header to disk.
/// This is the true point at which the current transaction commits.
fn write_head(log: &Log) {
    let mut bcache = buf::buf_cache();

    let mut buf = bcache.get(log.dev, log.start as u32);
    buf.read();

    let lh_on_disk = unsafe {
        let ptr = buf.data_mut().as_mut_ptr().cast::<LogHeader>();
        &mut *ptr
    };

    lh_on_disk.n = log.lh.n;

    for i in 0..(log.lh.n) {
        lh_on_disk.block[i] = log.lh.block[i];
    }

    buf.write();
    bcache.release(buf);
}

/// Read the log header from disk into the in-memory log header
fn read_head(log: &mut Log) {
    let mut bcache = buf::buf_cache();

    let buf = bcache.get(log.dev, log.start as u32);

    let lh_on_disk = unsafe {
        let ptr = buf.data_mut().as_mut_ptr().cast::<LogHeader>();
        &mut *ptr
    };

    log.lh.init(lh_on_disk);

    bcache.release(buf);
}

/// Copy committed blocks from log to their home location.
fn install_trans(log: &Log) {
    let mut bcache = buf::buf_cache();

    for tail in 0..(log.lh.n) {
        let mut buf_to = bcache.get(log.dev, log.lh.block[tail]);
        buf_to.read();
        let mut buf_from = bcache.get(log.dev, (log.start + tail + 1) as u32);
        buf_from.read();

        unsafe {
            let dst = VirtAddr(buf_to.data().as_ptr() as u32);
            let src = VirtAddr(buf_from.data().as_ptr() as u32);
            let len = BLK_SIZE;
            util::memmove(dst, src, len);
        }

        buf_to.write();
        bcache.release(buf_from);
        bcache.release(buf_to);
    }
}

fn recover_from_log(log: &mut Log) {
    read_head(log);
    install_trans(log); // if committed, copy from log to disk
    log.lh.n = 0;
    write_head(log); // clear the log
}

/// Caller has modified b->data and is done with the buffer.
/// Record the block number and pin in the cache with B_DIRTY.
/// commit()/write_log() will do the disk write.
///
/// log_write() replaces bwrite(); a typical use is:
///   bp = bread(...)
///   modify bp->data[]
///   log_write(bp)
///   brelse(bp)
pub(crate) fn log_write(buf: &mut BufCacheHandler) {
    let mut log = get_log();

    if log.lh.n >= LOG_SIZE || log.lh.n >= log.size - 1 {
        panic!("too big a transaction");
    }
    if log.outstanding < 1 {
        panic!("log_write outside of trans");
    }

    let mut idx_opt = None;
    for i in 0..(log.lh.n) {
        if log.lh.block[i] == buf.blockno {
            idx_opt = Some(i);
            break;
        }
    }

    match idx_opt {
        Some(_) => {
            // do nothing
        }
        None => {
            // add new log
            let n = log.lh.n;
            log.lh.block[n] = buf.blockno;
            log.lh.n += 1;
        }
    }

    buf.make_dirty(); // prevent eviction
}

fn log_init(dev: u32) -> Log {
    if mem::size_of::<LogHeader>() >= BLK_SIZE {
        panic!("log_init: too big logheader");
    }

    let sb = superblock::get();
    let mut log = Log::new(sb.log_start as usize, sb.nlog as usize, dev);

    recover_from_log(&mut log);

    log
}
