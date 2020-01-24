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
