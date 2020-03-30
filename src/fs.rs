use crate::buf::buf_cache;
use crate::constants::*;
use crate::once::Once;
use crate::pmap::VirtAddr;
use crate::rwlock::{RwLock, RwLockUpgradeableGuard, RwLockWriteGuard};
use crate::spinlock::{Mutex, MutexGuard};
use crate::superblock::SuperBlock;
use crate::{buf, log, superblock, util};
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::mem;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileType {
    Empty,
    File,
    Dir,
}

/// in-memory copy of an inode
/// file.h in xv6
pub(crate) struct Inode {
    dev: u32,
    inum: u32,
    valid: bool, // already read data from disk
    // the below is same as DInode
    typ: FileType,
    major: u16,                // major device number (T_DEV only)
    minor: u16,                // minor device number (T_DEV only)
    nlink: u16,                // number of links to inode in file system
    size: u32,                 // size of file (bytes)
    addrs: [u32; NDIRECT + 1], // data block addresses
}

impl Inode {
    fn empty(dev: u32, inum: u32) -> Inode {
        Inode {
            dev,
            inum,
            valid: false,
            typ: FileType::Empty,
            major: 0,
            minor: 0,
            nlink: 0,
            size: 0,
            addrs: [0; NDIRECT + 1],
        }
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

// struct InodeCacheEntry {
//     dev: u32,
//     inum: u32,
//     ref_cnt: i32,
//     valid: i32,
//     inode: Arc<RwLock<Inode>>,
// }
//
// impl InodeCacheEntry {
//     const fn new() -> InodeCacheEntry {
//         InodeCacheEntry {
//             dev: 0,
//             inum: 0,
//             ref_cnt: 0,
//             valid: 0,
//             inode: Arc::new(RwLock::new(Inode::new())),
//         }
//     }
// }

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct InodeCacheKey {
    dev: u32,
    inum: u32,
}

impl InodeCacheKey {
    fn new() -> InodeCacheKey {
        InodeCacheKey { dev: 0, inum: 0 }
    }
}

struct InodeCache {
    inodes: BTreeMap<InodeCacheKey, Arc<RwLock<Inode>>>,
    n: usize,
}

impl InodeCache {
    fn new() -> InodeCache {
        InodeCache {
            inodes: BTreeMap::new(),
            n: 0,
        }
    }

    fn get(&self, dev: u32, inum: u32) -> Option<Arc<RwLock<Inode>>> {
        let key = InodeCacheKey { dev, inum };
        self.inodes.get(&key).map(|v| v.clone())
    }

    fn create(&mut self, dev: u32, inum: u32) -> Option<Arc<RwLock<Inode>>> {
        if self.n >= NINODE {
            return None;
        }
        let key = InodeCacheKey { dev, inum };
        self.inodes
            .insert(key, Arc::new(RwLock::new(Inode::empty(dev, inum))));
        self.n += 1;
        self.inodes.get(&key).map(|v| v.clone())
    }

    fn remove(&mut self, dev: u32, inum: u32) {
        let key = InodeCacheKey { dev, inum };
        self.inodes.remove(&key);
    }
}

static INODE_CACHE: Once<Mutex<InodeCache>> = Once::new();

/// Should call after kernel heap set up
fn inode_cache() -> &'static Mutex<InodeCache> {
    INODE_CACHE.call_once(|| Mutex::new(InodeCache::new()))
}

/// Return inode block corresponding to a passed inum.
fn block_for_inode(inum: u32, sb: &SuperBlock) -> u32 {
    inum / (IPB as u32) + sb.inode_start
}

/// Allocate an inode on device dev.
pub(crate) fn ialloc(dev: u32, typ: FileType) -> Arc<RwLock<Inode>> {
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

/// Find the inode with number inum on device dev
/// and return the in-memory copy. Does not lock
/// the inode and does not read it from disk.
fn iget(dev: u32, inum: u32) -> Arc<RwLock<Inode>> {
    let mut icache = inode_cache().lock();

    match icache.get(dev, inum) {
        Some(ip) => ip,
        None => match icache.create(dev, inum) {
            Some(ip) => ip,
            None => panic!("iget: no inodes"),
        },
    }
}

/// Increment reference count for ip.
/// Returns ip to enable ip = idup(ip1) idiom.
pub(crate) fn idup(ip: &Arc<RwLock<Inode>>) -> Arc<RwLock<Inode>> {
    Arc::clone(ip)
}

/// Copy a modified in-memory inode to disk.
/// Must be called after every change to an ip->xxx field
/// that lives on disk, since i-node cache is write-through.
/// Caller must hold ip->lock.
pub(crate) fn iupdate(inode: &Inode) {
    let sb = superblock::get();

    let mut bcache = buf::buf_cache();
    let mut bp = bcache.get(inode.dev, block_for_inode(inode.inum, sb));
    bp.read();

    let dinode = unsafe { &mut *bp.data_mut().as_mut_ptr().cast::<DInode>() };
    dinode.typ = inode.typ;
    dinode.major = inode.major;
    dinode.minor = inode.minor;
    dinode.nlink = inode.nlink;
    dinode.size = inode.size;
    unsafe {
        println!("size_of(ip.addrs): {}", mem::size_of_val(&inode.addrs));
        util::memmove(
            VirtAddr(dinode.addrs.as_ptr() as u32),
            VirtAddr(inode.addrs.as_ptr() as u32),
            mem::size_of_val(&inode.addrs),
        )
    };
    log::log_write(&mut bp);

    bcache.release(bp);
}

/// Lock the given inode.
/// Reads the inode from disk if necessary.
pub(crate) fn ilock(ip: &Arc<RwLock<Inode>>) -> RwLockWriteGuard<'_, Inode> {
    let sb = superblock::get();
    let ip = &**ip;

    let mut lk = ip.write();

    // read data from disk
    let inode = &mut *lk;
    if !inode.valid {
        let mut bcache = buf::buf_cache();
        let mut bp = bcache.get(inode.dev, block_for_inode(inode.inum, sb));
        bp.read();
        let dinode = unsafe { &*bp.data().as_ptr().cast::<DInode>() };

        inode.typ = dinode.typ;
        inode.major = dinode.major;
        inode.minor = dinode.minor;
        inode.nlink = dinode.nlink;
        inode.size = dinode.size;
        unsafe {
            println!("size_of(ip.addrs): {}", mem::size_of_val(&inode.addrs));
            util::memmove(
                VirtAddr(inode.addrs.as_ptr() as u32),
                VirtAddr(dinode.addrs.as_ptr() as u32),
                mem::size_of_val(&inode.addrs),
            )
        };
        inode.valid = true;

        bcache.release(bp);

        if inode.typ == FileType::Empty {
            panic!("ilock: no type");
        }
    }

    lk
}

/// Unlock the given inode.
pub(crate) fn iunlock(inode: RwLockWriteGuard<'_, Inode>) {
    // just consume RwLockWriteGuard
}

/// Drop a reference to an in-memory inode.
/// If that was the last reference, the inode cache entry can
/// be recycled.
/// If that was the last reference and the inode has no links
/// to it, free the inode (and its content) on disk.
/// All calls to iput() must be inside a transaction in
/// case it has to free the inode.
pub(crate) fn iput(ip: Arc<RwLock<Inode>>) {
    let mut lk = ip.write();
    // Someone might have Arc<RwLock<Inode>>, but no one can see Inode for a while.

    let inode = &mut *lk;

    // FIXME: Is it possible that Someone having Arc<RwLock<Inode>> get into trouble?
    if inode.valid && inode.nlink == 0 {
        let mut icache = inode_cache().lock();

        itrunc(inode);
        inode.typ = FileType::Empty;
        iupdate(inode);
        inode.valid = false;

        icache.remove(inode.dev, inode.inum);
    }
}

/// Calculate a bitmap brock appropriate for blockno
fn block_for_bitmap(blockno: u32, sb: &SuperBlock) -> u32 {
    blockno / (BPB as u32) + sb.bmap_start
}

/// Free a disk block
fn bfree(dev: u32, blockno: u32) {
    let sb = superblock::get();
    let mut bcache = buf::buf_cache();

    let mut bp = bcache.get(dev, block_for_bitmap(blockno, sb));
    bp.read();

    let bi = blockno % (BPB as u32);
    let m = 1 << (bi % 8);
    if bp.data()[(bi / 8) as usize] & m == 0 {
        panic!("bfree: freeing free block");
    }
    bp.data_mut()[(bi / 8) as usize] &= !m;
    log::log_write(&mut bp);

    bcache.release(bp);
}

// Truncate inode (discard contents).
// Only called when the inode has no links
// to it (no directory entries referring to it)
// and has no in-memory reference to it (is
// not an open file or current directory).
fn itrunc(inode: &mut Inode) {
    for i in 0..NDIRECT {
        if inode.addrs[i] > 0 {
            bfree(inode.dev, inode.addrs[i]);
            inode.addrs[i] = 0;
        }
    }

    if inode.addrs[NDIRECT] > 0 {
        // there are indirect inodes too.
        let mut bcache = buf::buf_cache();
        let mut bp = bcache.get(inode.dev, inode.addrs[NDIRECT]);
        bp.read();

        let a = bp.data().as_ptr().cast::<u32>();
        for i in 0..NINDIRECT {
            let inum = unsafe { *a.add(i) };
            if inum > 0 {
                bfree(inode.dev, inum);
            }
        }

        bcache.release(bp);
        bfree(inode.dev, inode.addrs[NDIRECT]);
        inode.addrs[NDIRECT] = 0;
    }

    inode.size = 0;
    iupdate(inode);
}

/// Common idiom: unlock, then put.
pub(crate) fn iunlockput(ip: Arc<RwLock<Inode>>, inode: RwLockWriteGuard<'_, Inode>) {
    let lk = iunlock(inode);
    iput(ip);
}
