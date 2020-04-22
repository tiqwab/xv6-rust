// This file comes from kern/syscall.c in jos. See COPYRIGHT for copyright information.

use crate::env::EnvId;
use crate::file::FileDescriptor;
use crate::fs::Stat;
use crate::pmap::VirtAddr;
use crate::sched;
use crate::{env, sysfile};
use alloc::vec::Vec;
use consts::*;
use core::ptr::null;
use core::slice;
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

/// Dispatched to the correct kernel function, passing the arguments.
pub(crate) unsafe fn syscall(syscall_no: u32, a1: u32, a2: u32, a3: u32, a4: u32, a5: u32) -> i32 {
    if syscall_no == SYS_CPUTS {
        let raw_s = a1 as *const u8;
        let len = a2 as usize;
        let curenv = env::cur_env_mut().expect("curenv should be exist");

        env::user_mem_assert(curenv, VirtAddr(raw_s as u32), len, 0);

        let s = {
            let utf8s = slice::from_raw_parts(raw_s, len);
            str::from_utf8(utf8s).expect("illegal utf8 string")
        };
        sys_cputs(s);
        0
    } else if syscall_no == SYS_EXIT {
        let _status = a1 as i32;
        let curenv = env::cur_env_mut().expect("curenv should be exist");
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
        sysfile::exec(path, &arg_vec[..]).unwrap_or_else(|err| {
            println!("Error occurred: {}", sysfile::str_error(err));
        });
        0
    } else if syscall_no == SYS_OPEN {
        let path = a1 as *const u8;
        let mode = a2 as u32;
        let res = sysfile::open(path, mode)
            .map(|fd| fd.0 as i32)
            .unwrap_or_else(|err| {
                println!("Error occurred: {}", sysfile::str_error(err));
                -1
            });
        res
    } else if syscall_no == SYS_CLOSE {
        let fd = FileDescriptor(a1 as u32);
        sysfile::close(fd).unwrap_or_else(|err| {
            println!("Error occurred: {}", sysfile::str_error(err));
        });
        0
    } else if syscall_no == SYS_READ {
        let fd = FileDescriptor(a1 as u32);
        let buf = a2 as *mut u8;
        let count = a3 as usize;
        match env::cur_env_mut().unwrap().fd_get(fd) {
            None => {
                println!("Error occurred in read: fd {} not found", fd.0);
                -1
            }
            Some(ent) => {
                let mut f = ent.file.write();
                f.read(buf, count).map(|cnt| cnt as i32).unwrap_or_else(|| {
                    println!("Error occurred in read: failed to read fd {}", fd.0);
                    -1
                })
            }
        }
    } else if syscall_no == SYS_WRITE {
        let fd = FileDescriptor(a1 as u32);
        let buf = a2 as *mut u8;
        let count = a3 as usize;
        match env::cur_env_mut().unwrap().fd_get(fd) {
            None => {
                println!("Error occurred in read: fd {} not found", fd.0);
                -1
            }
            Some(ent) => {
                let mut f = ent.file.write();
                f.write(buf, count)
                    .map(|cnt| cnt as i32)
                    .unwrap_or_else(|| {
                        println!("Error occurred in write: failed to write fd {}", fd.0);
                        -1
                    })
            }
        }
    } else if syscall_no == SYS_MKNOD {
        let path = a1 as *const u8;
        let major = a2 as u16;
        let minor = a2 as u16;
        sysfile::mknod(path, major, minor).unwrap_or_else(|err| {
            println!("Error occurred: {}", sysfile::str_error(err));
        });
        0
    } else if syscall_no == SYS_DUP {
        let fd = FileDescriptor(a1 as u32);
        sysfile::dup(fd)
            .map(|fd| fd.0 as i32)
            .unwrap_or_else(|err| {
                println!("Error occurred: {}", sysfile::str_error(err));
                -1
            })
    } else if syscall_no == SYS_WAIT_ENV_ID {
        let env_id = EnvId(a1);
        match env::wait_env_id(env_id) {
            None => 0,
            Some(id) => id.0 as i32,
        }
    } else if syscall_no == SYS_SBRK {
        let nbytes = a1 as usize;
        let p = env::sbrk(nbytes);
        if p.is_null() {
            -1
        } else {
            p as i32
        }
    } else if syscall_no == SYS_FSTAT {
        let fd = FileDescriptor(a1);
        let statbuf = &mut *(a2 as *mut Stat);
        match sysfile::stat(fd) {
            Err(err) => {
                println!("Error occurred: {}", sysfile::str_error(err));
                -1
            }
            Ok(stat) => {
                *statbuf = stat;
                0
            }
        }
    } else if syscall_no == SYS_GETCWD {
        let buf = a1 as *mut u8;
        let size = a2 as usize;
        match sysfile::getcwd(buf, size) {
            Err(err) => {
                println!("Error occurred: {}", sysfile::str_error(err));
                -1
            }
            Ok(len) => len as i32,
        }
    } else {
        panic!("unknown syscall");
    }
}
