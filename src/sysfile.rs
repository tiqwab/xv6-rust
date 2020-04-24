use crate::constants::*;
use crate::file::{FileDescriptor, FileTableEntry};
use crate::fs::{DirEnt, Inode, InodeType, Stat};
use crate::pmap::VirtAddr;
use crate::rwlock::{RwLock, RwLockWriteGuard};
use crate::{env, file, fs, log, util};
use alloc::sync::Arc;
use consts::*;
use core::ops::Try;
use core::ptr::{null, null_mut, slice_from_raw_parts};
use core::{cmp, mem};

pub(crate) mod consts {
    pub(crate) const O_RDONLY: u32 = 0x000;
    pub(crate) const O_WRONLY: u32 = 0x001;
    pub(crate) const O_RDWR: u32 = 0x002;
    pub(crate) const O_CREATE: u32 = 0x200;
}

// Create the path new as a link to the same inode as old.
pub(crate) fn link(new: *const u8, old: *const u8) -> Result<(), SysError> {
    log::begin_op();

    let ip = fs::namei(old).into_result().map_err(|_| {
        log::end_op();
        SysError::NoEnt
    })?;

    let mut inode = fs::ilock(&ip);
    let inode_dev = inode.get_dev();
    let inode_inum = inode.get_inum();

    if inode.is_dir() {
        fs::iunlock(inode);
        log::end_op();
        return Err(SysError::IsDir);
    }

    inode.incr_nlink();
    fs::iupdate(&inode);
    fs::iunlock(inode);

    fn when_err(ip: Arc<RwLock<Inode>>) {
        let mut inode = fs::ilock(&ip);
        inode.decr_nlink();
        fs::iupdate(&inode);
        fs::iunlock(inode);
        fs::iput(ip);
        log::end_op();
    }

    let mut name = [0; DIR_SIZ];
    let res = fs::nameiparent(new, name.as_mut_ptr())
        .into_result()
        .map_err(|_| SysError::InvalidArg)
        .and_then(|dp| {
            let mut dir_inode = fs::ilock(&dp);
            if dir_inode.get_dev() == inode_dev
                && fs::dir_link(&mut dir_inode, name.as_ptr(), inode_inum)
            {
                fs::iunlock(dir_inode);
                fs::iput(dp);
                Ok(())
            } else {
                fs::iunlock(dir_inode);
                fs::iput(dp);
                Err(SysError::InvalidArg)
            }
        });

    match res {
        Ok(_) => {
            fs::iput(ip);
            log::end_op();
            Ok(())
        }
        Err(err) => {
            when_err(ip);
            Err(err)
        }
    }
}

pub(crate) fn unlink(path: *const u8) -> Result<(), SysError> {
    log::begin_op();

    let mut name = [0; DIR_SIZ];

    // get inode for the directory
    let dp = fs::nameiparent(path, name.as_mut_ptr())
        .into_result()
        .map_err(|_| {
            log::end_op();
            SysError::InvalidArg
        })?;

    let mut dir_inode = fs::ilock(&dp);

    // cannot unlink "." or ".."
    if util::strncmp(name.as_ptr(), ".".as_ptr(), DIR_SIZ) == 0
        || util::strncmp(name.as_ptr(), "..".as_ptr(), DIR_SIZ) == 0
    {
        fs::iunlock(dir_inode);
        log::end_op();
        return Err(SysError::InvalidArg);
    }

    let mut off = 0;

    // get the target inode in the directory
    let ip = fs::dir_lookup_with_name(&mut dir_inode, name.as_ptr(), &mut off)
        .into_result()
        .map_err(|_| SysError::NoEnt);

    let ip = match ip {
        Ok(inode) => inode,
        Err(err) => {
            fs::iunlock(dir_inode);
            fs::iput(dp);
            log::end_op();
            return Err(err);
        }
    };

    let mut inode = fs::ilock(&ip);

    if inode.get_nlink() < 1 {
        panic!("unlink: nlink < 1");
    }
    if inode.is_dir() && fs::is_dir_empty(&mut inode) {
        fs::iunlock(inode);
        fs::iunlock(dir_inode);
        fs::iput(dp);
        log::end_op();
        return Err(SysError::InvalidArg);
    }

    // Remove the inode from the dir
    let ent = DirEnt::empty();
    let ent_p = &ent as *const _ as *const u8;
    let dir_ent_size = mem::size_of::<DirEnt>() as u32;
    let n = fs::writei(&mut dir_inode, ent_p, off, dir_ent_size);
    if n != dir_ent_size {
        panic!("unlink: failed to writei");
    }

    if inode.is_dir() {
        dir_inode.decr_nlink();
        fs::iupdate(&dir_inode);
    }
    fs::iunlock(dir_inode);
    fs::iput(dp);

    inode.decr_nlink();
    fs::iupdate(&inode);
    fs::iunlock(inode);
    fs::iput(ip);

    log::end_op();
    Ok(())
}

fn create(
    path: *const u8,
    typ: InodeType,
    major: u16,
    minor: u16,
) -> Result<Arc<RwLock<Inode>>, SysError> {
    let mut name = [0; DIR_SIZ];

    let dp = fs::nameiparent(path, name.as_mut_ptr())
        .into_result()
        .map_err(|_| {
            log::end_op();
            SysError::InvalidArg
        })?;

    let mut dir_inode = fs::ilock(&dp);

    let ip = fs::dir_lookup_with_name(&mut dir_inode, name.as_ptr(), null_mut());
    let ip = match ip {
        Some(p) => {
            fs::iunlock(dir_inode);
            fs::iput(dp);
            let inode = fs::ilock(&p);
            if typ == InodeType::File && inode.is_file() {
                fs::iunlock(inode);
                return Ok(p);
            } else {
                fs::iunlock(inode);
                return Err(SysError::IsDir);
            }
        }
        None => fs::ialloc(dir_inode.get_dev(), typ, major, minor),
    };

    let mut inode = fs::ilock(&ip);
    fs::iupdate(&inode);

    if typ == InodeType::Dir {
        // create "." and ".." entries
        dir_inode.incr_nlink();
        fs::iupdate(&dir_inode);
        // no ip->nlink++ for "."; avoid cyclic ref count.
        let inum1 = inode.get_inum();
        let inum2 = dir_inode.get_inum();
        let dot1 = ['.' as u8, 0];
        let dot2 = ['.' as u8, '.' as u8, 0];
        if !fs::dir_link(&mut inode, dot1.as_ptr(), inum1)
            || !fs::dir_link(&mut inode, dot2.as_ptr(), inum2)
        {
            panic!("create: failed to create dots");
        }
    }

    if !fs::dir_link(&mut dir_inode, name.as_ptr(), inode.get_inum()) {
        panic!("create: failed to dir_link");
    }

    fs::iunlock(inode);

    Ok(ip)
}

/// Allocate a file descriptor for the given file.
/// Takes over file reference from caller on success.
/// Return the passed ent when an error occurred.
fn fd_alloc(ent: FileTableEntry) -> Result<FileDescriptor, FileTableEntry> {
    let cur_env = env::cur_env_mut().unwrap();
    cur_env.fd_alloc(ent)
}

pub(crate) fn open(path: *const u8, mode: u32) -> Result<FileDescriptor, SysError> {
    log::begin_op();

    let ip = if mode & O_CREATE != 0 {
        match create(path, InodeType::File, 0, 0) {
            Ok(ip) => Ok(ip),
            Err(err) => {
                log::end_op();
                Err(err)
            }
        }
    } else {
        match fs::namei(path) {
            Some(ip) => Ok(ip),
            None => {
                log::end_op();
                Err(SysError::NoEnt)
            }
        }
    }?;

    let inode = fs::ilock(&ip);

    if inode.is_dir() && mode != O_RDONLY {
        fs::iunlock(inode);
        fs::iput(ip);
        log::end_op();
        return Err(SysError::IsDir);
    }

    let mut ft = file::file_table();
    let readable = mode & O_WRONLY == 0;
    let writable = (mode & O_WRONLY != 0) || (mode & O_RDWR != 0);

    match ft.alloc_as_inode(readable, writable, &ip) {
        None => {
            fs::iunlock(inode);
            fs::iput(ip);
            log::end_op();
            Err(SysError::TooManyFiles)
        }
        Some(ent) => {
            let fd_opt = fd_alloc(ent);
            match fd_opt {
                Err(ent) => {
                    ft.close(ent);
                    fs::iunlock(inode);
                    fs::iput(ip);
                    log::end_op();
                    Err(SysError::TooManyFileDescriptors)
                }
                Ok(fd) => {
                    fs::iunlock(inode);
                    log::end_op();
                    Ok(fd)
                }
            }
        }
    }
}

pub(crate) fn close(fd: FileDescriptor) -> Result<(), SysError> {
    let ent = env::cur_env_mut().unwrap().fd_close(fd);
    file::file_table().close(ent);
    Ok(())
}

pub(crate) fn mkdir(path: *const u8) -> Result<(), SysError> {
    log::begin_op();
    let res = create(path, InodeType::Dir, 0, 0).map(|_| ());
    log::end_op();
    res
}

pub(crate) fn mknod(path: *const u8, major: u16, minor: u16) -> Result<(), SysError> {
    log::begin_op();
    let res = create(path, InodeType::Dev, major, minor).map(|_| ());
    log::end_op();
    res
}

pub(crate) fn stat(fd: FileDescriptor) -> Result<Stat, SysError> {
    match env::cur_env_mut().unwrap().fd_get(fd) {
        None => Err(SysError::IllegalFileDescriptor),
        Some(ent) => match ent.file.read().stat() {
            None => Err(SysError::IllegalFileDescriptor),
            Some(stat) => Ok(stat),
        },
    }
}

pub(crate) fn dup(fd: FileDescriptor) -> Result<FileDescriptor, SysError> {
    let env = env::cur_env_mut().unwrap();
    env.fd_get(fd)
        .into_result()
        .map_err(|_| SysError::IllegalFileDescriptor)?;
    match env.fd_dup(fd) {
        None => Err(SysError::TooManyFileDescriptors),
        Some(fd) => Ok(fd),
    }
}

pub(crate) fn chdir(path: *const u8) -> Result<(), SysError> {
    let cur_env = env::cur_env_mut().unwrap();

    log::begin_op();

    let ip = match fs::namei(path) {
        Some(ip) => ip,
        None => {
            log::end_op();
            return Err(SysError::NoEnt);
        }
    };

    let inode = fs::ilock(&ip);

    if !inode.is_dir() {
        fs::iunlock(inode);
        log::end_op();
        return Err(SysError::NotDir);
    }

    fs::iunlock(inode);

    cur_env.change_cwd(&ip);
    log::end_op();

    Ok(())
}

pub(crate) fn exec(orig_path: *const u8, orig_argv: &[*const u8]) -> Result<(), SysError> {
    let env = env::cur_env_mut().unwrap();

    unsafe {
        // copy path and argv because they are in user space.
        let path = [0 as u8; DIR_SIZ];
        let dst = VirtAddr(&path as *const _ as *const u8 as u32);
        let src = VirtAddr(orig_path as *const u8 as u32);
        util::memcpy(dst, src, DIR_SIZ);

        let mut argv_stack = [[0 as u8; MAX_CMD_ARG_LEN]; MAX_CMD_ARGS];
        for (i, s) in orig_argv.iter().enumerate() {
            let len = util::strnlen(*s, MAX_CMD_ARG_LEN);
            util::strncpy(argv_stack[i].as_mut_ptr(), *s, len + 1);
        }

        let mut argv = [null() as *const u8; MAX_CMD_ARGS];
        for i in 0..orig_argv.len() {
            argv[i] = argv_stack[i].as_ptr() as *const u8;
        }

        env::exec(path.as_ptr(), &argv[0..orig_argv.len()], env);
    }
    Ok(())
}

/// Return the length of name (exclusive '\0' at the end)
pub(crate) fn getcwd(buf: *mut u8, size: usize) -> Result<usize, SysError> {
    // Call f recursively to create an absolute path for cwd
    fn f(
        cur: Arc<RwLock<Inode>>,
        mut len: usize,
        buf: *mut u8,
        buf_size: usize,
    ) -> Result<usize, SysError> {
        let mut cur_ip = cur.write();
        let cur_inum = cur_ip.get_inum();

        if cur_ip.get_dev() == ROOT_DEV && cur_ip.get_inum() == ROOT_INUM {
            return Ok(len);
        }

        let path_parent = [b'.', b'.', b'\0'];
        let parent = fs::dir_lookup_with_name(&mut cur_ip, path_parent.as_ptr(), null_mut())
            .into_result()
            .map_err(|_| SysError::NoEnt)?;

        len = f(parent.clone(), len, buf, buf_size)?;

        let mut parent_ip = parent.write();

        let mut off: u32 = 0;
        fs::dir_lookup_with_inum(&mut parent_ip, cur_inum, &mut off);

        let mut ent = DirEnt::empty();
        let dir_ent_size = mem::size_of::<DirEnt>();
        let cnt = fs::readi(
            &mut parent_ip,
            &mut ent as *mut _ as *mut u8,
            off,
            dir_ent_size as u32,
        );

        if cnt != dir_ent_size as u32 {
            Err(SysError::Unspecified)
        } else {
            // add '/'
            let slash = [b'/', b'\0'];
            len = append(slash.as_ptr(), len, buf, buf_size);
            // add dir name
            len = append(ent.get_name(), len, buf, buf_size);
            Ok(len)
        }
    }

    // Append name to the last of buf.
    // Return new len.
    fn append(name: *const u8, len: usize, buf: *mut u8, buf_size: usize) -> usize {
        let name_len = util::strnlen(name, DIR_SIZ);
        let added_len = cmp::max(0, cmp::min(buf_size - len - 1, name_len));
        unsafe { util::strncpy(buf.add(len), name, added_len) };
        len + added_len
    }

    let env = env::cur_env().unwrap();
    let res = f(env.get_cwd().clone(), 0, buf, size);
    res.map(|mut len| {
        if len == 0 {
            // this is root
            let slash = [b'/', b'\0'];
            len = append(slash.as_ptr(), len, buf, size);
        }
        unsafe { *buf.add(len) = 0 };
        len
    })
}
