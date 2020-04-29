use crate::pmap::VirtAddr;

pub(crate) unsafe fn memset(va: VirtAddr, c: u8, n: usize) {
    let mut p = va.0 as *mut u8;
    for _ in 0..n {
        *p = c;
        p = p.add(1);
    }
}

pub(crate) unsafe fn memcpy(dest: VirtAddr, src: VirtAddr, n: usize) {
    let mut p_dest = dest.0 as *mut u8;
    let mut p_src = src.0 as *mut u8;
    for _ in 0..n {
        *p_dest = *p_src;
        p_dest = p_dest.add(1);
        p_src = p_src.add(1);
    }
}

pub(crate) unsafe fn memmove(dest: VirtAddr, src: VirtAddr, n: usize) {
    memcpy(dest, src, n);
}

pub(crate) fn strnlen(s: *const u8, max_len: usize) -> usize {
    unsafe {
        let mut p = s;
        let mut i = 0;
        while i < max_len {
            if *p == 0 {
                break;
            }
            i += 1;
            p = p.add(1);
        }
        i
    }
}

pub(crate) fn strncmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    unsafe {
        let mut p1 = s1;
        let mut p2 = s2;
        for _ in 0..n {
            let c1 = *p1;
            let c2 = *p2;
            if c1 == 0 && c2 == 0 {
                break;
            } else if c1 > c2 {
                return 1;
            } else if c1 < c2 {
                return -1;
            }
            p1 = p1.add(1);
            p2 = p2.add(1);
        }
        0
    }
}

pub(crate) fn strncpy(mut dst: *mut u8, mut src: *const u8, n: usize) -> *mut u8 {
    unsafe {
        let ret = dst;
        let mut cnt = 0;
        while cnt < n && *src != 0 {
            *dst = *src;
            dst = dst.add(1);
            src = src.add(1);
            cnt += 1;
        }
        ret
    }
}
