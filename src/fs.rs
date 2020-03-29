use crate::buf::buf_cache;
use crate::constants::*;
use crate::pmap::VirtAddr;
use crate::rwlock::{RwLockUpgradeableGuard, RwLockWriteGuard};
use crate::spinlock::{Mutex, MutexGuard};
use crate::superblock::SuperBlock;
use crate::{buf, log, superblock, util};
use core::mem;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileType {
    Empty,
    File,
    Dir,
}

/// in-memory copy of an inode
/// file.h in xv6
pub(crate) struct Inode {}

impl Inode {
    const fn new() -> Inode {
        Inode {}
    }
}

/// On-disk inode structure
/// fs.h in xv6
pub(crate) struct DInode {
    typ: FileType,
    major: u16,                // major device number (T_DEV only)
    minor: u16,                // minor device number (T_DEV only)
    nlink: u16,                // number of links to inode in file system
    size: u32,                 // size of file (bytes)
    addrs: [u32; NDIRECT + 1], // data block addresses
}

struct InodeCache {
    inodes: [Inode; NINODE],
}

impl InodeCache {
    const fn new() -> InodeCache {
        InodeCache {
            inodes: [Inode::new(); NINODE],
        }
    }
}

static INODE_CACHE: Mutex<InodeCache> = Mutex::new(InodeCache::new());

/// Return inode block corresponding to a passed inum.
fn block_for_inode(inum: u32, sb: &SuperBlock) -> u32 {
    inum / (IPB as u32) + sb.inode_start
}

/// Allocate an inode on device dev.
/// TODO: want to return ReadWriteLockGuard
pub(crate) fn ialloc(dev: u32, typ: FileType) -> RwLockUpgradeableGuard<'static, Inode> {
    let sb = superblock::get();

    for inum in 1..(sb.ninodes) {
        let mut bcache = buf::buf_cache();
        let mut bp = bcache.get(dev, block_for_inode(inum, sb));
        bp.read();

        let data = bp.data_mut().as_mut_ptr();
        let dip = unsafe { &mut *data.cast::<DInode>() };
        if dip.typ == FileType::Empty {
            // a free node
            unsafe { util::memset(VirtAddr(data as u32), 0, mem::size_of::<DInode>()) };
            dip.typ = typ;
            log::log_write(&mut bp); // mark it allocated on the disk
            bcache.release(bp);
            return iget(dev, inum);
        }

        bcache.release(bp);
    }

    panic!("ialloc: no inodes");
}

/// Copy a modified in-memory inode to disk.
/// Must be called after every change to an ip->xxx field
/// that lives on disk, since i-node cache is write-through.
/// Caller must hold ip->lock.
pub(crate) fn iupdate(ip: &Inode) {
    unimplemented!()
}

/// Find the inode with number inum on device dev
/// and return the in-memory copy. Does not lock
/// the inode and does not read it from disk.
fn iget(dev: u32, inum: u32) -> RwLockUpgradeableGuard<'static, Inode> {
    unimplemented!()
}

/// Increment reference count for ip.
/// Returns ip to enable ip = idup(ip1) idiom.
pub(crate) fn idup(ip: &Inode) {
    unimplemented!()
}

/// Lock the given inode.
/// Reads the inode from disk if necessary.
pub(crate) fn ilock(ip: RwLockUpgradeableGuard<'_, Inode>) -> RwLockWriteGuard<'_, Inode> {
    // Take write lock and fetch data.

    unimplemented!()
}

/// Unlock the given inode.
pub(crate) fn iunlock(ip: RwLockWriteGuard<'_, Inode>) -> RwLockUpgradeableGuard<'_, Inode> {
    unimplemented!()
}

/// Drop a reference to an in-memory inode.
/// If that was the last reference, the inode cache entry can
/// be recycled.
/// If that was the last reference and the inode has no links
/// to it, free the inode (and its content) on disk.
/// All calls to iput() must be inside a transaction in
/// case it has to free the inode.
pub(crate) fn iput(ip: RwLockUpgradeableGuard<'_, Inode>) {
    // Take write lock

    // Release write lock

    unimplemented!()
}

/// Common idiom: unlock, then put.
pub(crate) fn iunlockput(ip: RwLockWriteGuard<'_, Inode>) {
    let lk = iunlock(ip);
    iput(lk);
}
