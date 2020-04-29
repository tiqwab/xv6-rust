// This file comes from kern/syscall.c in jos. See COPYRIGHT for copyright information.

use crate::constants::{SysError, MAX_PATH_LEN, PTE_W};
use crate::env::EnvId;
use crate::file::FileDescriptor;
use crate::fs::Stat;
use crate::pmap::VirtAddr;
use crate::{env, sysfile};
use crate::{sched, util};
use alloc::vec::Vec;
use consts::*;
use core::mem;
use core::ptr::null;
use core::str;

mod consts {
    pub(crate) static SYS_CPUTS: u32 = 0;
    pub(crate) static SYS_GETC: u32 = 1;
    pub(crate) static SYS_EXIT: u32 = 2;
    pub(crate) static SYS_YIELD: u32 = 3;
    pub(crate) static SYS_GET_ENV_ID: u32 = 4;
    pub(crate) static SYS_FORK: u32 = 5;
    pub(crate) static SYS_KILL: u32 = 6;
    pub(crate) static SYS_EXEC: u32 = 7;
    pub(crate) static SYS_OPEN: u32 = 8;
    pub(crate) static SYS_CLOSE: u32 = 9;
    pub(crate) static SYS_READ: u32 = 10;
    pub(crate) static SYS_WRITE: u32 = 11;
    pub(crate) static SYS_MKNOD: u32 = 12;
    pub(crate) static SYS_DUP: u32 = 13;
    pub(crate) static SYS_WAIT_ENV_ID: u32 = 14;
    pub(crate) static SYS_SBRK: u32 = 15;
    pub(crate) static SYS_FSTAT: u32 = 16;
    pub(crate) static SYS_GETCWD: u32 = 17;
    pub(crate) static SYS_MKDIR: u32 = 18;
    pub(crate) static SYS_CHDIR: u32 = 19;
    pub(crate) static SYS_PIPE: u32 = 20;
}

pub(crate) fn str_error(err: SysError) -> &'static str {
    match err {
        SysError::Unspecified => "unexpected error",
        SysError::NoEnt => "no such file or directory",
        SysError::IsDir => "is a directory",
        SysError::NotDir => "is not a directory",
        SysError::InvalidArg => "invalid argument",
        SysError::TooManyFiles => "open too many files",
        SysError::TooManyFileDescriptors => "open too many file descriptors",
        SysError::IllegalFileDescriptor => "illegal file descriptor",
        SysError::TryAgain => "try again",
        SysError::BrokenPipe => "broken pipe",
        SysError::NotChild => "not child process",
    }
}

fn sys_cputs(s: &str) {
    print!("{}", s);
}

fn sys_yield() {
    sched::sched_yield();
}

fn sys_get_env_id() -> EnvId {
    let cur_env = env::cur_env().unwrap();
    cur_env.get_env_id()
}

fn sys_fork() -> EnvId {
    let cur_env = env::cur_env_mut().unwrap();
    env::fork(cur_env)
}

fn sys_write(fd: FileDescriptor, buf: *const u8, len: usize) -> i32 {
    match env::cur_env_mut().unwrap().fd_get(fd) {
        None => SysError::IllegalFileDescriptor.err_no(),
        Some(ent) => {
            let mut f = ent.file.write();
            match f.write(buf, len) {
                Err(err) => err.err_no(),
                Ok(cnt) => cnt as i32,
            }
        }
    }
}

/// Check a system call argument for path.
/// It should be in user space and less than MAX_CMD_ARG_LEN.
/// If check fails, the functino doesn't return.
fn path_check(arg: *const u8) {
    let curenv = env::cur_env_mut().expect("curenv should be exist");
    let len = util::strnlen(arg, MAX_PATH_LEN + 1);
    if len > MAX_PATH_LEN {
        let env_table = env::env_table();
        env::env_destroy(curenv.get_env_id(), env_table);
    }
    env::user_mem_assert(curenv, VirtAddr(arg as u32), len, 0);
}

/// Dispatched to the correct kernel function, passing the arguments.
pub(crate) unsafe fn syscall(syscall_no: u32, a1: u32, a2: u32, a3: u32, a4: u32, a5: u32) -> i32 {
    if syscall_no == SYS_CPUTS {
        // SYS_CPUTS is deprecated, use SYS_WRITE instead.
        let raw_s = a1 as *const u8;
        let len = a2 as usize;
        let curenv = env::cur_env_mut().expect("curenv should exist");
        env::user_mem_assert(curenv, VirtAddr(raw_s as u32), len, 0);
        sys_write(FileDescriptor(1), raw_s, len)
    } else if syscall_no == SYS_EXIT {
        let _status = a1 as i32;
        let curenv = env::cur_env_mut().expect("curenv should exist");
        #[cfg(feature = "debug")]
        println!("[{:08x}] exiting gracefully", curenv.get_env_id());
        let env_table = env::env_table();
        env::env_destroy(curenv.get_env_id(), env_table);
        0
    } else if syscall_no == SYS_YIELD {
        sys_yield();
        0
    } else if syscall_no == SYS_GET_ENV_ID {
        let env_id = sys_get_env_id();
        env_id.0 as i32
    } else if syscall_no == SYS_FORK {
        let env_id = sys_fork();
        env_id.0 as i32
    } else if syscall_no == SYS_KILL {
        let env_id = EnvId(a1);
        let env_table = env::env_table();
        env::env_destroy(env_id, env_table);
        0
    } else if syscall_no == SYS_EXEC {
        let path = a1 as *const u8;
        path_check(path);
        let arg_arr = [
            a2 as *const u8,
            a3 as *const u8,
            a4 as *const u8,
            a5 as *const u8,
        ];
        let mut arg_vec = Vec::new();
        for v in arg_arr.iter() {
            if *v != null() {
                arg_vec.push(*v);
            }
        }
        match sysfile::exec(path, &arg_vec[..]) {
            Err(err) => err.err_no(),
            Ok(_) => 0,
        }
    } else if syscall_no == SYS_OPEN {
        let path = a1 as *const u8;
        path_check(path);
        let mode = a2 as u32;
        sysfile::open(path, mode)
            .map(|fd| fd.0 as i32)
            .unwrap_or_else(|err| err.err_no())
    } else if syscall_no == SYS_CLOSE {
        let fd = FileDescriptor(a1 as u32);
        match sysfile::close(fd) {
            Err(err) => err.err_no(),
            Ok(_) => 0,
        }
    } else if syscall_no == SYS_READ {
        let fd = FileDescriptor(a1 as u32);
        let buf = a2 as *mut u8;
        let count = a3 as usize;

        let curenv = env::cur_env_mut().expect("curenv should exist");
        env::user_mem_assert(curenv, VirtAddr(buf as u32), count, 0);

        match env::cur_env_mut().unwrap().fd_get(fd) {
            None => SysError::IllegalFileDescriptor.err_no(),
            Some(ent) => {
                let mut f = ent.file.write();
                match f.read(buf, count) {
                    Err(err) => err.err_no(),
                    Ok(cnt) => cnt as i32,
                }
            }
        }
    } else if syscall_no == SYS_WRITE {
        let fd = FileDescriptor(a1 as u32);
        let buf = a2 as *mut u8;
        let count = a3 as usize;

        let curenv = env::cur_env_mut().expect("curenv should exist");
        env::user_mem_assert(curenv, VirtAddr(buf as u32), count, PTE_W);

        sys_write(fd, buf, count)
    } else if syscall_no == SYS_MKNOD {
        let path = a1 as *const u8;
        path_check(path);
        let major = a2 as u16;
        let minor = a2 as u16;
        match sysfile::mknod(path, major, minor) {
            Err(err) => err.err_no(),
            Ok(_) => 0,
        }
    } else if syscall_no == SYS_DUP {
        let fd = FileDescriptor(a1 as u32);
        sysfile::dup(fd)
            .map(|fd| fd.0 as i32)
            .unwrap_or_else(|err| err.err_no())
    } else if syscall_no == SYS_WAIT_ENV_ID {
        let env_id = EnvId(a1);
        match env::wait_env_id(env_id) {
            Err(err) => err.err_no(),
            Ok(id) => id.0 as i32,
        }
    } else if syscall_no == SYS_SBRK {
        let nbytes = a1 as usize;
        let p = env::sbrk(nbytes);
        if p.is_null() {
            SysError::Unspecified.err_no()
        } else {
            p as i32
        }
    } else if syscall_no == SYS_FSTAT {
        let fd = FileDescriptor(a1);
        let statbuf = {
            let p = a2 as *mut Stat;
            let curenv = env::cur_env_mut().expect("curenv should exist");
            let len = mem::size_of::<Stat>();
            env::user_mem_assert(curenv, VirtAddr(p as u32), len, PTE_W);
            &mut *p
        };
        match sysfile::stat(fd) {
            Err(err) => err.err_no(),
            Ok(stat) => {
                *statbuf = stat;
                0
            }
        }
    } else if syscall_no == SYS_GETCWD {
        let buf = a1 as *mut u8;
        let size = a2 as usize;

        let curenv = env::cur_env_mut().expect("curenv should exist");
        env::user_mem_assert(curenv, VirtAddr(buf as u32), size, PTE_W);

        match sysfile::getcwd(buf, size) {
            Err(err) => err.err_no(),
            Ok(len) => len as i32,
        }
    } else if syscall_no == SYS_MKDIR {
        let path = a1 as *const u8;
        path_check(path);
        match sysfile::mkdir(path) {
            Err(err) => err.err_no(),
            Ok(_) => 0,
        }
    } else if syscall_no == SYS_CHDIR {
        let path = a1 as *const u8;
        path_check(path);
        match sysfile::chdir(path) {
            Err(err) => err.err_no(),
            Ok(_) => 0,
        }
    } else if syscall_no == SYS_PIPE {
        let fds = &mut *(a1 as *mut [FileDescriptor; 2]);
        match sysfile::pipe() {
            Err(err) => err.err_no(),
            Ok((fd0, fd1)) => {
                fds[0] = fd0;
                fds[1] = fd1;
                0
            }
        }
    } else {
        panic!("unknown syscall");
    }
}
