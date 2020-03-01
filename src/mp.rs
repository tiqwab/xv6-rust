use crate::constants::*;
use crate::pmap::{PhysAddr, VirtAddr};
use crate::{gdt, lapic, mpconfig, pmap, trap, util};

extern "C" {
    static mpentry_start: u32;
    static mpentry_end: u32;
}

#[no_mangle]
pub static mut mpentry_kstack: u32 = 0;

/// Start the non-boot (AP) processors.
/// This function is expected to be executed by BSP (Bootstrap Processor).
pub(crate) fn boot_aps() {
    // Write entry code to unused memory at MPENTRY_PADDR
    let entry_start = unsafe { &mpentry_start as *const _ as u32 };
    let entry_end = unsafe { &mpentry_end as *const _ as u32 };
    let entry_len = (entry_end - entry_start) as usize;
    assert!(
        entry_len <= PGSIZE as usize,
        "entry code for mp is too large"
    );
    let code = PhysAddr(MPENTRY_PADDR).to_va();
    unsafe { util::memmove(code, VirtAddr(entry_start), entry_len) };

    // Boot each AP one at a time
    let stacks = pmap::percpu_kstacks();
    let bsp = mpconfig::this_cpu();
    for cpu in mpconfig::cpus() {
        if cpu.cpu_id == bsp.cpu_id {
            continue; // skip because we've already started.
        }

        println!("Start initializing CPU({})", cpu.cpu_id);

        // Tell mpetnry.S what stack to use
        let stack_for_cpu = unsafe { &mut *(&mut mpentry_kstack as *mut u32) };
        *stack_for_cpu = (stacks[cpu.cpu_id as usize].as_ptr() as u32) + KSTKSIZE;

        // Start the CPU at mpentry_start
        lapic::startap(cpu.cpu_id, code.to_pa());

        // Wait for the CPU to finish some basic setup in mp_main()
        while !cpu.is_started() {}

        println!("Finish initializing CPU({})", cpu.cpu_id);
    }
}

/// Setup code for APs
#[no_mangle]
pub extern "C" fn mp_main() {
    // We are in high EIP now, safe to switch to kern_pgdir
    pmap::load_kern_pgdir();
    let cpu = mpconfig::this_cpu_mut();
    println!("SMP: CPU {} starting", cpu.cpu_id);

    lapic::lapic_init();
    unsafe { gdt::init_percpu() };
    unsafe { trap::trap_init_percpu() };

    cpu.started();

    // TODO
    // lock_kernel(); // unlock in shed_halt
    // sched_yield();

    loop {}
}
