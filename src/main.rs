#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let vga_buffer = 0xb8000 as *mut u8;
    unsafe {
        // cannot use data section for now
        *vga_buffer.offset(0) = 'h' as u8;
        *vga_buffer.offset(1) = 0xb;
        *vga_buffer.offset(2) = 'e' as u8;
        *vga_buffer.offset(3) = 0xb;
        *vga_buffer.offset(4) = 'l' as u8;
        *vga_buffer.offset(5) = 0xb;
        *vga_buffer.offset(6) = 'l' as u8;
        *vga_buffer.offset(7) = 0xb;
        *vga_buffer.offset(8) = 'o' as u8;
        *vga_buffer.offset(9) = 0xb;
        *vga_buffer.offset(10) = '!' as u8;
        *vga_buffer.offset(11) = 0xb;
    }

    loop {}
}
