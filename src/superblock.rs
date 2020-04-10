use crate::buf;
use crate::once::Once;

// Disk layout:
// [ boot block | super block | log | inode blocks | free bit map | data blocks ]
//
// mkfs computes the super block and builds an initial file system.
// The super block describes the disk layout.
#[repr(C)]
pub(crate) struct SuperBlock {
    pub(crate) size: u32,        // size of file system image (blocks)
    pub(crate) nblocks: u32,     // number of data blocks
    pub(crate) ninodes: u32,     // number of inodes
    pub(crate) nlog: u32,        // number of log blocks
    pub(crate) log_start: u32,   // block number of the first log block
    pub(crate) inode_start: u32, // block number of the first inode block
    pub(crate) bmap_start: u32,  // block number of the first free bit map block
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

static SUPER_BLOCK: Once<SuperBlock> = Once::new();

fn read_sb(dev: u32) -> SuperBlock {
    let mut sb = SuperBlock::empty();

    let mut bcache = buf::buf_cache();
    let mut b = bcache.get(dev, 1);
    b.read();
    let data = b.data();

    let disk_sb = unsafe { &*data.as_ptr().cast::<SuperBlock>() };
    sb.init(disk_sb);
    println!(
        "log_start: {}, inode_start: {}, bmap_start: {}",
        sb.log_start, sb.inode_start, sb.bmap_start
    );

    bcache.release(b);

    sb
}

/// Should be called only after ide_init and buf_init
pub(crate) fn get() -> &'static SuperBlock {
    SUPER_BLOCK.call_once(|| read_sb(1))
}
