use crate::constants::*;
use crate::file::{FileDescriptor, FileTableEntry};
use crate::fs::{DirEnt, Inode, InodeType};
use crate::rwlock::{RwLock, RwLockWriteGuard};
use crate::{env, file, fs, log, util};
use alloc::sync::Arc;
use consts::*;
use core::mem;
use core::ops::Try;
use core::ptr::{null, null_mut, slice_from_raw_parts};

pub(crate) mod consts {
    pub(crate) const O_RDONLY: u32 = 0x000;
    pub(crate) const O_WRONLY: u32 = 0x001;
    pub(crate) const O_RDWR: u32 = 0x002;
    pub(crate) const O_CREATE: u32 = 0x200;
}

pub(crate) enum SysFileError {
    IllegalFileName,
    TooManyFiles,
    TooManyFileDescriptors,
}

// Create the path new as a link to the same inode as old.
pub(crate) fn link(new: *const u8, old: *const u8) -> Result<(), SysFileError> {
    log::begin_op();

    let ip = fs::namei(old).into_result().map_err(|_| {
        log::end_op();
        SysFileError::IllegalFileName
    })?;

    let mut inode = fs::ilock(&ip);
    let inode_dev = inode.get_dev();
    let inode_inum = inode.get_inum();

    if inode.is_dir() {
        fs::iunlock(inode);
        log::end_op();
        return Err(SysFileError::IllegalFileName);
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
        .map_err(|_| SysFileError::IllegalFileName)
        .and_then(|dp| {
            let mut dir_inode = fs::ilock(&dp);
            let name_str = core::str::from_utf8(&name).unwrap();
            if dir_inode.get_dev() == inode_dev
                && fs::dir_link(&mut dir_inode, name_str, inode_inum)
            {
                fs::iunlock(dir_inode);
                fs::iput(dp);
                Ok(())
            } else {
                fs::iunlock(dir_inode);
                fs::iput(dp);
                Err(SysFileError::IllegalFileName)
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

pub(crate) fn unlink(path: *const u8) -> Result<(), SysFileError> {
    log::begin_op();

    let mut name = [0; DIR_SIZ];

    // get inode for the directory
    let dp = fs::nameiparent(path, name.as_mut_ptr())
        .into_result()
        .map_err(|_| {
            log::end_op();
            SysFileError::IllegalFileName
        })?;

    let mut dir_inode = fs::ilock(&dp);

    // cannot unlink "." or ".."
    if util::strncmp(name.as_ptr(), ".".as_ptr(), DIR_SIZ) == 0
        || util::strncmp(name.as_ptr(), "..".as_ptr(), DIR_SIZ) == 0
    {
        fs::iunlock(dir_inode);
        log::end_op();
        return Err(SysFileError::IllegalFileName);
    }

    let mut off = 0;
    let name_str = core::str::from_utf8(&name).unwrap();

    // get the target inode in the directory
    let ip = fs::dir_lookup(&mut dir_inode, name_str, &mut off)
        .into_result()
        .map_err(|_| SysFileError::IllegalFileName);

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
        return Err(SysFileError::IllegalFileName);
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
) -> Result<Arc<RwLock<Inode>>, SysFileError> {
    let mut name = [0; DIR_SIZ];

    let dp = fs::nameiparent(path, name.as_mut_ptr())
        .into_result()
        .map_err(|_| {
            log::end_op();
            SysFileError::IllegalFileName
        })?;

    let mut dir_inode = fs::ilock(&dp);
    let name_str = core::str::from_utf8(&name).unwrap();

    let ip = fs::dir_lookup(&mut dir_inode, name_str, null_mut());
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
                return Err(SysFileError::IllegalFileName);
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
        let inum = inode.get_inum();
        if !fs::dir_link(&mut inode, ".", inum) || !fs::dir_link(&mut inode, "..", inum) {
            panic!("create: failed to create dots");
        }
    }

    if !fs::dir_link(&mut dir_inode, name_str, inode.get_inum()) {
        panic!("create: failed to dir_link");
    }

    fs::iunlock(inode);
    Ok(ip)
}

/// Allocate a file descriptor for the given file.
// Takes over file reference from caller on success.
fn fd_alloc(ent: &FileTableEntry) -> Option<FileDescriptor> {
    let cur_env = env::cur_env_mut().unwrap();
    cur_env.fd_alloc(&ent.file)
}

pub(crate) fn open(path: *const u8, mode: u32) -> Result<FileDescriptor, SysFileError> {
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
                Err(SysFileError::IllegalFileName)
            }
        }
    }?;

    let inode = fs::ilock(&ip);

    if inode.is_dir() && mode != O_RDONLY {
        fs::iunlock(inode);
        fs::iput(ip);
        log::end_op();
        return Err(SysFileError::IllegalFileName);
    }

    let mut ft = file::file_table();
    let readable = mode & O_WRONLY == 0;
    let writable = (mode & O_WRONLY != 0) || (mode & O_RDWR != 0);

    let f_opt = ft.alloc_as_inode(readable, writable, &ip);
    let fd_opt = f_opt.as_ref().and_then(|f| fd_alloc(f));
    let (_f, fd) = match (f_opt, fd_opt) {
        (Some(f), Some(fd)) => (f, fd),
        (Some(f), _) => {
            ft.close(f);
            fs::iunlock(inode);
            fs::iput(ip);
            log::end_op();
            return Err(SysFileError::TooManyFileDescriptors);
        }
        (_, _) => {
            fs::iunlock(inode);
            fs::iput(ip);
            log::end_op();
            return Err(SysFileError::TooManyFiles);
        }
    };

    fs::iunlock(inode);
    log::end_op();

    Ok(fd)
}

pub(crate) fn mkdir(path: *const u8) -> Result<(), SysFileError> {
    log::begin_op();
    let res = create(path, InodeType::Dir, 0, 0).map(|_| ());
    log::end_op();
    res
}

pub(crate) fn mknod(path: *const u8, major: u16, minor: u16) -> Result<(), SysFileError> {
    log::begin_op();
    let res = create(path, InodeType::Dev, major, minor).map(|_| ());
    log::end_op();
    res
}

pub(crate) fn chdir(path: *const u8) -> Result<(), SysFileError> {
    let cur_env = env::cur_env_mut().unwrap();

    log::begin_op();

    let ip = match fs::namei(path) {
        Some(ip) => ip,
        None => {
            log::end_op();
            return Err(SysFileError::IllegalFileName);
        }
    };

    let inode = fs::ilock(&ip);

    if !inode.is_dir() {
        fs::iunlock(inode);
        log::end_op();
        return Err(SysFileError::IllegalFileName);
    }

    fs::iunlock(inode);

    cur_env.change_cwd(&ip);
    log::end_op();

    Ok(())
}
