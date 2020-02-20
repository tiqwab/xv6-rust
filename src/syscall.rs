// This file comes from kern/syscall.c in jos. See COPYRIGHT for copyright information.

use crate::env;
use crate::pmap::VirtAddr;
use consts::*;
use core::slice;
use core::str;

mod consts {
    pub(crate) static SYS_CPUTS: u32 = 0;
}

fn sys_cputs(s: &str) {
    print!("{}", s);
}

/// Dispatched to the correct kernel function, passing the arguments.
pub(crate) unsafe fn syscall(
    syscall_no: u32,
    a1: u32,
    a2: u32,
    _a3: u32,
    _a4: u32,
    _a5: u32,
) -> i32 {
    if syscall_no == SYS_CPUTS {
        let raw_s = a1 as *const u8;
        let len = a2 as usize;
        let curenv = env::cur_env().expect("curenv should be exist");

        env::user_mem_assert(curenv, VirtAddr(raw_s as u32), len, 0);

        let s = {
            let utf8s = slice::from_raw_parts(raw_s, len);
            str::from_utf8(utf8s).expect("illegal utf8 string")
        };
        sys_cputs(s);
        0
    } else {
        panic!("unknown syscall");
    }
}
