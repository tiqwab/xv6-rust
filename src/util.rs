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
