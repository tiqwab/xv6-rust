#![no_std]
#![no_main]
#![feature(const_fn)]

use xv6_rust::lib_main;
use xv6_rust::println;

#[no_mangle]
pub extern "C" fn i386_init() -> ! {
    lib_main();
    test_hello();
    loop {}
}

fn test_hello() {
    println!("hello from test");
}
