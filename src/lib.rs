#![no_std]
#![feature(const_fn)]
#![feature(asm)]

pub mod console;
pub mod serial;
pub mod vga_buffer;
pub mod volatile;
mod x86;

use core::panic::PanicInfo;
use vga_buffer::Buffer;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    loop {}
}

#[no_mangle]
pub fn lib_main() {
    let vga_buffer = unsafe { &mut *(0xb8000 as *mut Buffer) };
    vga_buffer::init_writer(vga_buffer);

    serial::init_serial();

    print!("H");
    println!("ello");
    println!("The numbers are {} and {}", 42, 1.0 / 3.0);
}
