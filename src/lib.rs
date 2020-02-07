#![no_std]
#![feature(const_fn)]
#![feature(asm)]
#![feature(bool_to_option)]
#![feature(ptr_offset_from)]
#![feature(alloc_error_handler)]
#![feature(const_in_array_repeat_expressions)]
#![feature(const_raw_ptr_to_usize_cast)]
#![feature(thread_local)]
// FIXME: remove later
#![allow(dead_code)]

// This must come first to resolve macro?
#[macro_use]
pub mod console;

mod allocator;
pub mod constants;
mod env;
mod gdt;
mod kclock;
mod pmap;
pub mod serial;
mod trap;
pub mod vga_buffer;
pub mod volatile;
mod x86;

extern crate alloc;
extern crate linked_list_allocator;

use crate::allocator::HeapAllocator;
use crate::env::EnvType;
use constants::*;
use core::panic::PanicInfo;
use vga_buffer::Buffer;

#[global_allocator]
static ALLOCATOR: allocator::HeapAllocator = allocator::HeapAllocator;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    loop {}
}

#[alloc_error_handler]
fn alloc_error_handler(layout: alloc::alloc::Layout) -> ! {
    panic!("allocation error: {:?}", layout);
}

#[no_mangle]
pub fn lib_main() {
    let vga_buffer = unsafe { &mut *((0xb8000 + KERN_BASE) as *mut Buffer) };
    vga_buffer::init_writer(vga_buffer);

    serial::init_serial();

    pmap::mem_init();

    unsafe { HeapAllocator::init(KHEAP_BASE as usize, KHEAP_SIZE) };

    {
        let mut a = alloc::boxed::Box::new(1);
        *a += 1;
        println!("a = {}", *a);
    }

    gdt::init_percpu();

    env::env_create(EnvType::User);

    print!("H");
    println!("ello");
    println!("The numbers are {} and {}", 42, 1.0 / 3.0);
}
