#![no_std]
#![no_main]
#![feature(const_fn)]

use xv6_rust::lib_main;
use xv6_rust::println;

extern "C" {
    fn my_add(x: i32, y: i32) -> i32;
    fn my_mul(x: i32, y: i32) -> i32;
    fn my_nop();
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    lib_main();
    unsafe {
        println!("1 + 2 = {}", my_add(1, 2));
        println!("2 * 3 = {}", my_mul(2, 3));
        my_nop();
    }
    loop {}
}
