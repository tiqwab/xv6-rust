use crate::buf::{buf_cache, BufCache, BufCacheHandler};
use crate::constants::*;
use crate::once::Once;
use crate::pmap::VirtAddr;
use crate::rwlock::{RwLock, RwLockUpgradeableGuard, RwLockWriteGuard};
use crate::spinlock::{Mutex, MutexGuard};
use crate::superblock::SuperBlock;
use crate::{buf, device, env, file, log, superblock, util};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::cmp::min;
use core::mem;
use core::ptr::{null, null_mut, slice_from_raw_parts};

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub(crate) enum InodeType {
    Empty,
    Dir,
    File,
    Dev,
}

/// in-memory copy of an inode
/// file.h in xv6
pub(crate) struct Inode {
    dev: u32,
    inum: u32,
    valid: bool, // already read data from disk
    // the below is same as DInode
    typ: InodeType,
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
            typ: InodeType::Empty,
            major: 0,
            minor: 0,
            nlink: 0,
            size: 0,
            addrs: [0; NDIRECT + 1],
        }
    }

    fn init(&mut self, major: u16, minor: u16) {
        self.major = major;
        self.minor = minor;
        self.nlink = 1;
    }

    pub(crate) fn is_dir(&self) -> bool {
        self.typ == InodeType::Dir
    }

    pub(crate) fn is_file(&self) -> bool {
        self.typ == InodeType::File
    }

    pub(crate) fn get_dev(&self) -> u32 {
        self.dev
    }

    pub(crate) fn get_inum(&self) -> u32 {
        self.inum
    }

    pub(crate) fn get_nlink(&self) -> u16 {
        self.nlink
    }

    pub(crate) fn incr_nlink(&mut self) {
        self.nlink += 1;
    }
    pub(crate) fn decr_nlink(&mut self) {
        self.nlink -= 1;
    }

    /// Return blockno of data at off bytes
    fn block_for(&mut self, off: u32, bcache: &mut BufCache) -> u32 {
        let mut off_as_blk = (off as usize) / BLK_SIZE;
        if off_as_blk < NDIRECT {
            if self.addrs[off_as_blk] == 0 {
                self.addrs[off_as_blk] = balloc(self.dev, bcache);
            }
            return self.addrs[off_as_blk];
        }

        off_as_blk -= NDIRECT;

        if off_as_blk < NINDIRECT {
            // load indirect block, allocating if necessary
            if self.addrs[NDIRECT] == 0 {
                self.addrs[NDIRECT] = balloc(self.dev, bcache);
            }

            let mut bp = bcache.get(self.dev, self.addrs[NDIRECT]);
            bp.read();

            let ap = unsafe { &mut *bp.data_mut().as_mut_ptr().cast::<u32>().add(off_as_blk) };
            if *ap == 0 {
                *ap = balloc(self.dev, bcache);
                log::log_write(&mut bp);
            }

            let block = *ap;
            bcache.release(bp);
            return block;
        }

        panic!("addr_for: out of range");
    }
}

/// On-disk inode structure
/// fs.h in xv6
#[repr(C)]
pub(crate) struct DInode {
    typ: InodeType,
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

/// Return inode pointer in the block.
/// Assume that a passed block is calculated correctly by block_for_inode.
fn ref_to_inode(inum: u32, bp: &mut BufCacheHandler) -> &mut DInode {
    let data = bp.data_mut().as_mut_ptr();
    let dip = data.cast::<DInode>();
    unsafe { &mut *dip.add((inum as usize) % IPB) }
}

/// Allocate an inode on device dev.
pub(crate) fn ialloc(dev: u32, typ: InodeType, major: u16, minor: u16) -> Arc<RwLock<Inode>> {
    let sb = superblock::get();

    for inum in 1..(sb.ninodes) {
        let mut bcache = buf::buf_cache();
        let mut bp = bcache.get(dev, block_for_inode(inum, sb));
        bp.read();

        let dinode = ref_to_inode(inum, &mut bp);
        if dinode.typ == InodeType::Empty {
            // a free node
            unsafe {
                util::memset(
                    VirtAddr(dinode as *const DInode as u32),
                    0,
                    mem::size_of::<DInode>(),
                )
            };
            dinode.typ = typ;
            dinode.major = major;
            dinode.minor = minor;
            dinode.nlink = 1;
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
pub(crate) fn iget(dev: u32, inum: u32) -> Arc<RwLock<Inode>> {
    let mut icache = inode_cache().lock();

    match icache.get(dev, inum) {
        Some(ip) => {
            #[cfg(feature = "debug")]
            println!("[iget] found inum {}", inum);
            ip
        }
        None => {
            #[cfg(feature = "debug")]
            println!("[iget] not found inum {}", inum);
            match icache.create(dev, inum) {
                Some(ip) => ip,
                None => panic!("iget: no inodes"),
            }
        }
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

    let dinode = ref_to_inode(inode.inum, &mut bp);
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

    // println!(
    //     "[ilock] inode {} is in block {}",
    //     inode.inum,
    //     block_for_inode(inode.inum, sb)
    // );

    if !inode.valid {
        let mut bcache = buf::buf_cache();
        let mut bp = bcache.get(inode.dev, block_for_inode(inode.inum, sb));
        bp.read();

        let dinode = ref_to_inode(inode.inum, &mut bp);

        inode.typ = dinode.typ;
        inode.major = dinode.major;
        inode.minor = dinode.minor;
        inode.nlink = dinode.nlink;
        inode.size = dinode.size;
        unsafe {
            util::memmove(
                VirtAddr(inode.addrs.as_ptr() as u32),
                VirtAddr(dinode.addrs.as_ptr() as u32),
                mem::size_of_val(&inode.addrs),
            )
        };
        inode.valid = true;

        bcache.release(bp);

        if inode.typ == InodeType::Empty {
            panic!("ilock: no type");
        }
    }

    lk
}

/// Unlock the given inode.
pub(crate) fn iunlock(_inode: RwLockWriteGuard<'_, Inode>) {
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
        inode.typ = InodeType::Empty;
        iupdate(inode);
        inode.valid = false;

        icache.remove(inode.dev, inode.inum);
    }
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
    iunlock(inode);
    iput(ip);
}

// ---------------------------------------------------------------------------------
// Inode Utility
// ---------------------------------------------------------------------------------

/// Read data from inode.
/// Return byte count of read data or None if read is not completed yet (it is possible in reading with device).
pub(crate) fn readi(inode: &mut Inode, mut dst: *mut u8, mut off: u32, mut n: u32) -> Option<u32> {
    if inode.typ == InodeType::Dev {
        let sw = device::get_dev_sw(CONSOLE).unwrap();
        let res: Option<i32> = sw.read.call((inode, dst, n as usize));
        return res.map(|cnt| cnt as u32);
    }

    if off > inode.size || off + n < off {
        panic!("readi: illegal offset");
    }
    if off + n > inode.size {
        n = inode.size - off;
    }

    #[cfg(feature = "debug")]
    println!("[readi] inum: {}, off: {}, n: {}", inode.inum, off, n);

    let mut bcache = buf::buf_cache();
    let mut tot = 0;
    while tot < n {
        let block = inode.block_for(off, &mut bcache);
        let mut bp = bcache.get(inode.dev, block);
        bp.read();

        let m = min(n - tot, (BLK_SIZE as u32) - off % (BLK_SIZE as u32));
        unsafe {
            util::memmove(
                VirtAddr(dst as u32),
                VirtAddr(bp.data().as_ptr().add((off as usize) % BLK_SIZE) as u32),
                m as usize,
            )
        };

        bcache.release(bp);
        tot += m;
        off += m;
        dst = unsafe { dst.add(m as usize) };
    }

    Some(n)
}

/// Write a data to inode.
/// Caller must hold ip->lock.
pub(crate) fn writei(inode: &mut Inode, mut src: *const u8, mut off: u32, n: u32) -> u32 {
    if inode.typ == InodeType::Dev {
        let sw = device::get_dev_sw(CONSOLE).unwrap();
        return sw.write.call((inode, src, n as usize)) as u32;
    }

    if off > inode.size || off + n < off {
        panic!("writei: illegal offset");
    }
    if off + n > (MAX_FILE * BLK_SIZE) as u32 {
        panic!("writei: too large offset");
    }

    println!("[writei] inum: {}, off: {}, n: {}", inode.inum, off, n);

    {
        let mut bcache = buf::buf_cache();
        let mut tot = 0;
        while tot < n {
            let block = inode.block_for(off, &mut bcache);
            let mut bp = bcache.get(inode.dev, block);
            bp.read();

            let m = min(n - tot, (BLK_SIZE as u32) - off % (BLK_SIZE as u32));
            unsafe {
                util::memmove(
                    VirtAddr(bp.data().as_ptr().add(off as usize % BLK_SIZE) as u32),
                    VirtAddr(src as u32),
                    m as usize,
                );
            }

            log::log_write(&mut bp);
            bcache.release(bp);
            tot += m;
            off += m;
            src = unsafe { src.add(m as usize) };
        }
    }

    if n > 0 && off > inode.size {
        inode.size = off;
        iupdate(inode);
    }

    n
}

// FIXME: the same definition is in user/stat.h
#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct Stat {
    pub(crate) typ: InodeType, // type of file
    pub(crate) dev: u32,       // file system's disk device
    pub(crate) inum: u32,      // inode number
    pub(crate) nlink: u16,     // number of links to file
    pub(crate) size: u32,      // size of file in bytes
}

pub(crate) fn stati(inode: &mut Inode) -> Stat {
    Stat {
        typ: inode.typ,
        dev: inode.dev,
        inum: inode.inum,
        nlink: inode.nlink,
        size: inode.size,
    }
}

// ---------------------------------------------------------------------------------
// Block handling
// ---------------------------------------------------------------------------------

/// Calculate a bitmap brock appropriate for blockno
fn block_for_bitmap(blockno: u32, sb: &SuperBlock) -> u32 {
    blockno / (BPB as u32) + sb.bmap_start
}

/// Allocate a zeroed disk block.
fn balloc(dev: u32, bcache: &mut BufCache) -> u32 {
    let sb = superblock::get();

    for blockno in 0..sb.size {
        let mut bp = bcache.get(dev, block_for_bitmap(blockno, sb));
        bp.read();

        let mut bi = 0;
        while bi < BPB && blockno + (bi as u32) < sb.size {
            let m = 1 << (bi % 8);
            // is block free?
            if bp.data()[bi / 8] & m == 0 {
                bp.data_mut()[bi / 8] |= m; // mark block in use
                log::log_write(&mut bp);
                bcache.release(bp);
                bzero(dev, blockno + (bi as u32), bcache);
                println!("[balloc] allocated blockno {}", blockno + (bi as u32));
                return blockno + (bi as u32);
            }
            bi += 1;
        }

        bcache.release(bp);
    }

    panic!("balloc: out of blocks");
}

/// Zero a block
fn bzero(dev: u32, blockno: u32, bcache: &mut BufCache) {
    let bp = bcache.get(dev, blockno);
    unsafe { util::memset(VirtAddr(bp.data().as_ptr() as u32), 0, BLK_SIZE) };
    bcache.release(bp);
}

/// Free a disk block
fn bfree(dev: u32, blockno: u32) {
    let sb = superblock::get();
    let mut bcache = buf::buf_cache();

    let mut bp = bcache.get(dev, block_for_bitmap(blockno, sb));
    bp.read();

    let bi = (blockno % (BPB as u32)) as usize;
    let m = 1 << (bi % 8);
    if bp.data()[bi / 8] & m == 0 {
        panic!("bfree: freeing free block");
    }
    bp.data_mut()[bi / 8] &= !m;
    log::log_write(&mut bp);

    bcache.release(bp);
}

// ---------------------------------------------------------------------------------
// Dir
// ---------------------------------------------------------------------------------

// FIXME: the same definition is in user/stat.h
#[repr(C)]
pub(crate) struct DirEnt {
    inum: u32,
    name: [u8; DIR_SIZ],
}

impl DirEnt {
    pub(crate) fn empty() -> DirEnt {
        DirEnt {
            inum: 0,
            name: [0; DIR_SIZ],
        }
    }

    fn set_name(&mut self, new_name: *const u8) {
        let new_len = util::strnlen(new_name, DIR_SIZ);
        if new_len > DIR_SIZ {
            panic!("DirEnt::set_name: too long name");
        }

        let mut dst = self.name.as_mut_ptr();
        let mut src = new_name;
        for i in 0..DIR_SIZ {
            unsafe {
                *dst = *src;
                dst = dst.add(1);
                src = src.add(1);
            }
        }
    }

    pub(crate) fn get_name(&self) -> *const u8 {
        self.name.as_ptr()
    }

    fn as_u8_ptr(&self) -> *const u8 {
        (self as *const DirEnt).cast::<u8>()
    }

    fn as_u8_mut_ptr(&mut self) -> *mut u8 {
        (self as *mut DirEnt).cast::<u8>()
    }
}

/// This is just for debug.
fn print_file_name(label: &str, p: *const u8) {
    let mut buf = [0 as u8; DIR_SIZ];
    for i in 0..(util::strnlen(p, DIR_SIZ)) {
        buf[i] = unsafe { *p.add(i) };
    }
    let sli = core::str::from_utf8(&buf).unwrap();
    println!("{}: {}", label, sli);
}

fn dir_lookup(
    dir: &mut Inode,
    p_off: *mut u32,
    cond: Box<dyn Fn(&DirEnt) -> bool>,
) -> Option<Arc<RwLock<Inode>>> {
    if dir.typ != InodeType::Dir {
        panic!("dir_lookup: inode is not dir");
    }

    let dir_ent_size = mem::size_of::<DirEnt>() as u32;
    let mut ent = DirEnt::empty();
    let mut off = 0;

    #[cfg(feature = "debug")]
    println!(
        "[dir_lookup] dir.inum: {}, dir.size: {}",
        dir.inum, dir.size
    );

    while off < dir.size {
        let ptr = ent.as_u8_mut_ptr();
        if readi(dir, ptr, off, dir_ent_size) != Some(dir_ent_size) {
            panic!("dir_lookup: failed to readi");
        }

        #[cfg(feature = "debug")]
        print!("[dir_lookup] ent.inum: {}, ", ent.inum);
        #[cfg(feature = "debug")]
        print_file_name("ent.name", ent.name.as_ptr());

        if ent.inum != 0 {
            if cond(&ent) {
                // entry matches path element
                if !p_off.is_null() {
                    unsafe { *p_off = off };
                }
                return Some(iget(dir.dev, ent.inum));
            }
        }

        off += dir_ent_size;
    }

    None
}

pub(crate) fn dir_lookup_with_name(
    dir: &mut Inode,
    name: *const u8,
    p_off: *mut u32,
) -> Option<Arc<RwLock<Inode>>> {
    #[cfg(feature = "debug")]
    print_file_name("dir_lookup for name", name);

    let cond: Box<dyn Fn(&DirEnt) -> bool> =
        Box::new(move |ent| util::strncmp(name, ent.name.as_ptr(), DIR_SIZ) == 0);
    dir_lookup(dir, p_off, Box::new(cond))
}

pub(crate) fn dir_lookup_with_inum(
    dir: &mut Inode,
    inum: u32,
    p_off: *mut u32,
) -> Option<Arc<RwLock<Inode>>> {
    #[cfg(feature = "debug")]
    println!("dir_lookup for inum: {}", inum);

    let cond: Box<dyn Fn(&DirEnt) -> bool> = Box::new(move |ent| ent.inum == inum);
    dir_lookup(dir, p_off, Box::new(cond))
}

/// Write a new directory entry (name, inum) into the directory dp.
/// Return true if successful. Return false if it already exists.
pub(crate) fn dir_link(dir: &mut Inode, name: *const u8, inum: u32) -> bool {
    // check that name is not present
    if dir_lookup_with_name(dir, name, null_mut()).is_some() {
        return false;
    }

    // look for an empty dirent
    let dir_ent_size = mem::size_of::<DirEnt>() as u32;
    let mut ent = DirEnt::empty();
    let mut off = 0;

    while off < dir.size {
        let ptr = ent.as_u8_mut_ptr();
        if readi(dir, ptr, off, dir_ent_size) != Some(dir_ent_size) {
            panic!("dir_link: failed to readi");
        }
        if ent.inum == 0 {
            break;
        }
        off += dir_ent_size;
    }

    ent.set_name(name);
    ent.inum = inum;
    let ptr = ent.as_u8_ptr();
    if writei(dir, ptr, off, dir_ent_size) != dir_ent_size {
        panic!("dir_link: failed to writei");
    }

    true
}

pub(crate) fn is_dir_empty(dp: &mut Inode) -> bool {
    assert!(dp.typ == InodeType::Dir);

    let dir_ent_size = mem::size_of::<DirEnt>() as u32;
    let mut off = 2 * dir_ent_size;

    while off < dp.size {
        let mut ent = DirEnt::empty();
        let ent_p = &mut ent as *mut _ as *mut u8;
        let n = readi(dp, ent_p, off, dir_ent_size);

        if n != Some(dir_ent_size) {
            panic!("is_dir_empty: failed to readi");
        }
        if ent.inum != 0 {
            return false;
        }

        off += dir_ent_size;
    }
    true
}

// ---------------------------------------------------------------------------------
// Path names
// ---------------------------------------------------------------------------------

/// Copy the next path element from path into name.
/// Return a pointer to the element following the copied one.
/// The returned path has no leading slashes,
/// so the caller can check *path=='\0' to see if the name is the last one.
/// If no name to remove, return 0.
///
/// Examples:
///   skipelem("a/bb/c", name) = "bb/c", setting name = "a"
///   skipelem("///a//bb", name) = "bb", setting name = "a"
///   skipelem("a", name) = "", setting name = "a"
///   skipelem("", name) = skipelem("////", name) = 0
unsafe fn skip_elem(mut path: *const u8, name: *mut u8) -> *const u8 {
    while *path == '/' as u8 {
        path = path.add(1);
    }
    if *path == 0 {
        return null();
    }

    let s = path;
    while *path != '/' as u8 && *path != 0 {
        path = path.add(1);
    }

    let len = path.offset_from(s);
    if len >= DIR_SIZ as isize {
        core::intrinsics::copy(s, name, DIR_SIZ);
    } else {
        core::intrinsics::copy(s, name, len as usize);
        *name.offset(len) = 0;
    }

    while *path == '/' as u8 {
        path = path.add(1);
    }
    path
}

/// Look up and return the inode for a path name.
/// If does_want_parent == true, return the inode for the parent and copy the final
/// path element into name, which must have room for DIRSIZ bytes.
/// Must be called inside a transaction since it calls iput().
fn namex(mut path: *const u8, does_want_parent: bool, name: *mut u8) -> Option<Arc<RwLock<Inode>>> {
    let mut ip: Arc<RwLock<Inode>>;

    unsafe {
        if *path == '/' as u8 {
            ip = iget(ROOT_DEV, ROOT_INUM);
        } else {
            let cur_env = env::cur_env().unwrap();
            ip = idup(cur_env.get_cwd())
        }

        loop {
            path = skip_elem(path, name);
            if path.is_null() {
                break;
            }

            let mut inode = ilock(&ip);

            if !inode.is_dir() {
                iunlock(inode);
                iput(ip);
                return None;
            }

            if does_want_parent && *path == '\0' as u8 {
                // stop one level early
                iunlock(inode);
                return Some(ip);
            }

            match dir_lookup_with_name(&mut inode, name, null_mut()) {
                None => {
                    iunlock(inode);
                    iput(ip);
                    return None;
                }
                Some(next) => {
                    iunlock(inode);
                    iput(ip);
                    ip = next;
                }
            }
        }

        if does_want_parent {
            iput(ip);
            return None;
        }
    }

    Some(ip)
}

pub(crate) fn namei(path: *const u8) -> Option<Arc<RwLock<Inode>>> {
    let mut name = [0; DIR_SIZ];
    namex(path, false, name.as_mut_ptr())
}

pub(crate) fn nameiparent(path: *const u8, name: *mut u8) -> Option<Arc<RwLock<Inode>>> {
    namex(path, true, name)
}

pub(crate) fn fs_test(dev: u32) {
    // Create a dir.
    //
    // Offset 0x4000 is start of inodes.
    // Size of DInode is 64 bytes.
    // Assigned inum was 3.
    //
    // ...
    // 0040c0 02 00 61 00 63 00 01 00 00 00 00 00 00 00 00 00  >..b.c...........<
    // 0040d0 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00  >................<
    // *
    log::begin_op();
    let idir = ialloc(dev, InodeType::Dir, 98, 99);
    let inum = idir.read().inum;
    {
        let idir = ilock(&idir);
        iupdate(&idir);
        iunlock(idir);
    }
    iput(idir);
    log::end_op();

    log::begin_op();
    let idir = iget(dev, inum);
    {
        let mut idir = ilock(&idir);
        idir.major -= 1;
        iupdate(&idir);
        iunlock(idir);
    }
    iput(idir);
    log::end_op();

    unsafe {
        //   skipelem("a/bb/c", name) = "bb/c", setting name = "a"
        //   skipelem("///a//bb", name) = "bb", setting name = "a"
        //   skipelem("a", name) = "", setting name = "a"
        //   skipelem("", name) = skipelem("////", name) = 0
        let path = "a/b//c";
        let mut name = [0; DIR_SIZ];
        let mut p = path.as_ptr();
        p = skip_elem(p, (&mut name[..]).as_mut_ptr());
        println!("path: {:p}, a: {:p}, name: {:?}", path, p, &name[..]);

        let path = "///a//bb";
        let mut name = [0; DIR_SIZ];
        let mut p = path.as_ptr();
        p = skip_elem(p, (&mut name[..]).as_mut_ptr());
        println!("path: {:p}, a: {:p}, name: {:?}", path, p, &name[..]);

        let path = ['a' as u8, '\0' as u8];
        let mut name = [0; DIR_SIZ];
        let mut p = path.as_ptr();
        p = skip_elem(p, (&mut name[..]).as_mut_ptr());
        println!(
            "path: {:p}, a: {:p}, name: {:?}",
            path.as_ptr(),
            p,
            &name[..]
        );

        let path = ['\0' as u8];
        let mut name = [0; DIR_SIZ];
        let mut p = path.as_ptr();
        p = skip_elem(p, (&mut name[..]).as_mut_ptr());
        println!(
            "path: {:p}, a: {:p}, name: {:?}",
            path.as_ptr(),
            p,
            &name[..]
        );
    }
}
