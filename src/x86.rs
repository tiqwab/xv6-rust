use crate::gdt::DescriptorTablePointer;
use crate::pmap::{PhysAddr, VirtAddr};

#[inline]
pub(crate) fn inb(port: u16) -> u8 {
    unsafe {
        let value: u8;
        asm!("inb $1, $0" : "={al}"(value) :"N{dx}"(port) :: "volatile");
        value
    }
}

#[inline]
pub(crate) fn insl(port: u16, addr: *mut u32, cnt: usize) {
    unsafe {
        asm!("cld; rep insl" : :
        "N{dx}" (port), "{esi}" (addr), "{ecx}" (cnt) :
        "memory", "cc" :
        "volatile");
    }

    // original in xv6
    // constraint D is di register
    // asm volatile("cld; rep insl" :
    //              "=D" (addr), "=c" (cnt) :
    //              "d" (port), "0" (addr), "1" (cnt) :
    //              "memory", "cc");
}

#[inline]
pub(crate) fn outb(port: u16, value: u8) {
    unsafe {
        asm!("outb $1, $0" :: "N{dx}"(port), "{al}"(value) :: "volatile");
    }
}

#[inline]
pub(crate) fn outsl(port: u16, addr: *const u32, cnt: usize) {
    unsafe {
        asm!("cld; rep outsl" : :
        "N{dx}" (port), "{esi}" (addr), "{ecx}" (cnt) :
        "cc" :
        "volatile");
    }

    // original in xv6
    // constraint S is si register
    // asm volatile("cld; rep outsl" :
    //              "=S" (addr), "=c" (cnt) :
    //              "d" (port), "0" (addr), "1" (cnt) :
    //              "cc");
}

#[inline]
pub(crate) fn rcr3() -> PhysAddr {
    let value: u32;
    unsafe { asm!("mov %cr3, $0" : "=r"(value) ::: "volatile") }
    PhysAddr(value)
}

#[inline]
pub(crate) fn lcr3(addr: PhysAddr) {
    unsafe { asm!("mov $0, %cr3" :: "r"(addr.0) : "memory" : "volatile") }
}

#[inline]
pub(crate) fn rcr0() -> u32 {
    let value: u32;
    unsafe { asm!("mov %cr0, $0" : "=r"(value) ::: "volatile") }
    value
}

#[inline]
pub(crate) fn lcr0(value: u32) {
    unsafe { asm!("mov $0, %cr0" :: "r"(value) : "memory" : "volatile") }
}

#[inline]
pub(crate) fn invlpg(va: VirtAddr) {
    unsafe { asm!("invlpg ($0)" :: "r"(va.0) : "memory" : "volatile") }
}

#[inline]
pub(crate) fn lgdt(p: &DescriptorTablePointer) {
    unsafe { asm!("lgdt ($0)" :: "r"(p) : "memory" : "volatile") }
}

#[inline]
pub(crate) fn lldt(p: &DescriptorTablePointer) {
    unsafe { asm!("lldt ($0)" :: "r"(p) : "memory" : "volatile") }
}

#[inline]
pub(crate) fn cld() {
    // The "cc" clobber indicates that the assembler code modifies the flags register
    unsafe { asm!("cld" ::: "cc" : "volatile") }
}

#[inline]
pub(crate) fn read_eflags() -> u32 {
    let value: u32;
    unsafe { asm!("pushfl; popl $0" : "=r" (value) ::: "volatile") }
    value
}

#[inline]
pub(crate) fn rcr2() -> u32 {
    let value: u32;
    unsafe { asm!("mov %cr2, $0" : "=r"(value) ::: "volatile") }
    value
}

#[inline]
pub(crate) fn ltr(selector: u16) {
    unsafe { asm!("ltr $0" :: "r"(selector) :: "volatile") }
}

#[inline]
pub(crate) fn lidt(p: &DescriptorTablePointer) {
    unsafe { asm!("lidt ($0)" :: "r"(p) : : "volatile") }
}

#[inline]
pub(crate) fn xchg<T>(p: *mut T, v: T) -> T {
    unsafe { core::intrinsics::atomic_xchg(p, v) }

    // Cannot work the below inline assembly...
    // It causes SIGSEGV when compiled.
    // ref. https://github.com/rust-lang/rust/issues/31437
    //
    // let res: u32;
    // unsafe { asm!("lock; xchgl $0, $1" : "+m"(*p), "=a"(res) : "1"(v) : "cc" : "volatile") }
    // res
}

#[inline]
pub(crate) fn cli() {
    unsafe { asm!("cli" :::: "volatile") };
}

#[inline]
pub(crate) fn sti() {
    unsafe { asm!("sti" :::: "volatile") };
}

#[inline]
pub(crate) fn pause() {
    unsafe { asm!("pause" :::: "volatile") };
}
