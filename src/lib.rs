#![no_std]
#![feature(const_fn)]
#![feature(bool_to_option)]
#![feature(ptr_offset_from)]
#![feature(alloc_error_handler)]
#![feature(const_in_array_repeat_expressions)]
#![feature(const_raw_ptr_deref)]
#![feature(const_raw_ptr_to_usize_cast)]
#![feature(thread_local)]
#![feature(core_intrinsics)]
#![feature(option_result_contains)]
#![feature(try_trait)]
#![feature(fn_traits)]
#![feature(llvm_asm)]
#![allow(dead_code)]

// This must come first to resolve macro?
#[macro_use]
pub mod console;

mod allocator;
mod buf;
pub mod constants;
mod device;
mod elf;
mod env;
mod file;
mod fs;
mod gdt;
mod ide;
mod kbd;
mod kclock;
mod kernel_lock;
mod lapic;
mod log;
mod mp;
mod mpconfig;
mod once;
mod picirq;
mod pipe;
mod pmap;
mod rwlock;
mod sched;
pub mod serial;
mod spinlock;
mod superblock;
mod syscall;
mod sysfile;
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
    unsafe {
        let vga_buffer = &mut *((0xb8000 + KERN_BASE) as *mut Buffer);
        vga_buffer::init_writer(vga_buffer);
        pmap::mem_init();
        HeapAllocator::init(KHEAP_BASE as usize, KHEAP_SIZE);
        gdt::init_percpu();
        trap::trap_init();
        mpconfig::mp_init();
        lapic::lapic_init();
        // do mp::boot_aps() after preparing processes
        picirq::pic_init();
        ide::ide_init();
        buf::buf_init();
        kbd::kbd_init();
        {
            let mut env_table = env::env_table();
            env::env_create_for_init(&mut env_table);
        }
        mp::boot_aps();
        sched::sched_yield();
    }
}
