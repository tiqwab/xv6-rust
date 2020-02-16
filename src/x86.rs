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
pub(crate) fn outb(port: u16, value: u8) {
    unsafe {
        asm!("outb $1, $0" :: "N{dx}"(port), "{al}"(value) :: "volatile");
    }
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
