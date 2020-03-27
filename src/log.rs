use crate::constants::{BLK_SIZE, LOG_SIZE};
use crate::spinlock::Mutex;
use crate::{buf, util};
use core::mem;

// Contents of the header block, used for both the on-disk header block
// and to keep track in memory of logged block# before commit.
//
// This is stored at the top of log blocks of disk
struct LogHeader {
    n: usize,
    block: [u32; LOG_SIZE],
}

struct Log {
    start: usize,
    size: usize,
    outstanding: usize, // how many FS sys calls are executing
    committing: bool,   // true if someone is in commit(). Please wait
    dev: u32,
    // lh: LogHeader,
}

impl Log {
    /// Create a new empty Log.
    /// It should be initialized with log_init.
    const fn empty() -> Log {
        Log {
            start: 0,
            size: 0,
            outstanding: 0,
            committing: false,
            dev: 0,
        }
    }
}

static LOG: Mutex<Log> = Mutex::new(Log::empty());

// Disk layout:
// [ boot block | super block | log | inode blocks | free bit map | data blocks ]
//
// mkfs computes the super block and builds an initial file system.
// The super block describes the disk layout.
//
// TODO: move somewhere
#[repr(C)]
struct SuperBlock {
    size: u32,        // size of file system image (blocks)
    nblocks: u32,     // number of data blocks
    ninodes: u32,     // number of inodes
    nlog: u32,        // number of log blocks
    log_start: u32,   // block number of the first log block
    inode_start: u32, // block number of the first inode block
    bmap_start: u32,  // block number of the first free bit map block
}

impl SuperBlock {
    /// Create a empty SuperBlock.
    /// It should be initialized with read_sb.
    const fn empty() -> SuperBlock {
        SuperBlock {
            size: 0,
            nblocks: 0,
            ninodes: 0,
            nlog: 0,
            log_start: 0,
            inode_start: 0,
            bmap_start: 0,
        }
    }

    fn init(&mut self, sb: &SuperBlock) {
        *self = SuperBlock {
            size: sb.size,
            nblocks: sb.nblocks,
            ninodes: sb.ninodes,
            nlog: sb.nlog,
            log_start: sb.log_start,
            inode_start: sb.inode_start,
            bmap_start: sb.bmap_start,
        };
    }
}

static SUPER_BLOCK: Mutex<SuperBlock> = Mutex::new(SuperBlock::empty());

fn read_sb(dev: u32) {
    let mut sb = SUPER_BLOCK.lock();

    let mut bcache = buf::buf_cache();
    let mut b = bcache.get(dev, 1);
    b.read();
    let data = b.data();

    let disk_sb = unsafe { &*data.as_ptr().cast::<SuperBlock>() };
    sb.init(disk_sb);

    bcache.release(dev, 1);
}

pub(crate) fn begin_op() {
    unimplemented!()
}

pub(crate) fn end_op() {
    unimplemented!()
}

pub(crate) fn log_init(dev: u32) {
    if mem::size_of::<LogHeader>() >= BLK_SIZE {
        panic!("log_init: too big logheader");
    }

    read_sb(dev);

    let sb = SUPER_BLOCK.lock();
    let mut log = LOG.lock();
    log.start = sb.log_start as usize;
    log.size = sb.nlog as usize;
    log.dev = dev;

    // recover_from_log();

    println!(
        "log_init: start = {}, size = {}, dev = {}",
        log.start, log.size, log.dev
    );
}
