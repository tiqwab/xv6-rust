#![no_std]
#![no_main]
#![feature(const_fn)]

use xv6_rust::lib_main;

#[no_mangle]
pub extern "C" fn i386_init() -> ! {
    lib_main();
    loop {}
}
