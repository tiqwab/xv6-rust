#![no_std]
#![feature(const_fn)]
#![feature(asm)]
#![feature(bool_to_option)]
#![feature(ptr_offset_from)]
#![feature(alloc_error_handler)]
#![feature(const_in_array_repeat_expressions)]
#![feature(const_raw_ptr_to_usize_cast)]
#![feature(thread_local)]
#![feature(slice_from_raw_parts)]
#![feature(core_intrinsics)]
#![feature(option_result_contains)]
// FIXME: remove later
#![allow(dead_code)]

// This must come first to resolve macro?
#[macro_use]
pub mod console;

mod allocator;
pub mod constants;
mod elf;
mod env;
mod gdt;
mod kclock;
mod kernel_lock;
mod lapic;
mod mp;
mod mpconfig;
mod pmap;
mod sched;
pub mod serial;
mod spinlock;
mod syscall;
mod trap;
mod util;
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

    unsafe {
        gdt::init_percpu();
    }
    unsafe {
        trap::trap_init();
    }

    unsafe {
        mpconfig::mp_init();
        lapic::lapic_init();
        mp::boot_aps();
    }

    print!("H");
    println!("ello");
    println!("The numbers are {} and {}", 42, 1.0 / 3.0);

    env::env_create_for_hello(EnvType::User);
    env::env_create_for_yield(EnvType::User);
    env::env_create_for_yield(EnvType::User);
    env::env_create_for_yield(EnvType::User);

    sched::sched_yield();
}
