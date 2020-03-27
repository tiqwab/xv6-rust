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
mod buf;
pub mod constants;
mod elf;
mod env;
mod gdt;
mod ide;
mod kclock;
mod kernel_lock;
mod lapic;
mod log;
mod mp;
mod mpconfig;
mod once;
mod picirq;
mod pmap;
mod rwlock;
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
        // do mp::boot_aps() after preparing processes
    }

    picirq::pic_init();

    ide::ide_init();
    buf::buf_init();
    log::log_init(1); // TODO: call it at the beginning of the first process execution (ref. forkret in xv6)

    print!("H");
    println!("ello");
    println!("The numbers are {} and {}", 42, 1.0 / 3.0);

    {
        let mut env_table = env::env_table();
        env::env_create_for_hello(&mut env_table);

        // env::env_create_for_yield(&mut env_table);
        // env::env_create_for_yield(&mut env_table);
        // env::env_create_for_yield(&mut env_table);

        // env::env_create_for_forktest(&mut env_table);

        env::env_create_for_spin(&mut env_table);
    }

    mp::boot_aps();

    sched::sched_yield();
}
