// ref. Intel SDM Vol.3 Chapter. 8 and 10 (APIC)

use crate::constants::*;
use crate::pmap::{PhysAddr, VirtAddr};
use crate::trap::consts::{IRQ_ERROR, IRQ_OFFSET, IRQ_SPURIOUS, IRQ_TIMER};
use crate::{kclock, mpconfig, pmap, x86};
use consts::*;

mod consts {
    // Local APIC registers, divided by 4 for use as uint32_t[] indices
    // ref. Intel SDM Vol.3 Table 10-1
    pub(crate) const ID: isize = 0x0020 / 4; // ID
    pub(crate) const VER: isize = 0x0030 / 4; // Version (read only)
    pub(crate) const TPR: isize = 0x0080 / 4; // Task Priority Register
    pub(crate) const APR: isize = 0x0090 / 4; // Arbitration Priority Register (read only)
    pub(crate) const EOI: isize = 0x00b0 / 4; // EOI (End Of Interrupt) register (write only)
    pub(crate) const SVR: isize = 0x00f0 / 4; // Spurious Interrupt Vector Register
    pub(crate) const ESR: isize = 0x0280 / 4; // Error Status (read only)
    pub(crate) const ICRLO: isize = 0x0300 / 4; // Interrupt Command [31:0]
    pub(crate) const ICRHI: isize = 0x0310 / 4; // Interrupt Command [63:32]
    pub(crate) const LVT_TIMER: isize = 0x0320 / 4; // Local Vector Table 0 (TIMER)
    pub(crate) const LVT_PC: isize = 0x0340 / 4; // Local Vector Table for Performance Monitoring Counter
    pub(crate) const LVT_LINT0: isize = 0x0350 / 4; // Local Vector Table 1 (LINT0)
    pub(crate) const LVT_LINT1: isize = 0x0360 / 4; // Local Vector Table 2 (LINT1)
    pub(crate) const LVT_ERROR: isize = 0x0370 / 4; // Local Vector Table 3 (ERROR)
    pub(crate) const TICR: isize = 0x0380 / 4; // Initial Count Register (for Timer)
    pub(crate) const TCCR: isize = 0x0390 / 4; // Current Count Register (for Timer) (read only)
    pub(crate) const TDCR: isize = 0x03E0 / 4; // Divide Configuration Register (for Timer)

    pub(crate) const SVR_ENABLE: i32 = 0x00000100; // Unit Enable

    // ref. Intel SDM Vol.3 Figure 10-8. Local Vector Table (LVT)
    pub(crate) const LVT_TIMER_PERIODIC: i32 = 0x00020000; // Periodic
    pub(crate) const LVT_MASKED: i32 = 0x00010000; // Interrupt masked. Inhibits reception of the interrupt if set.

    // ref. Intel SDM Vol.3 10.6.1 Interrupt Command Register ICR
    pub(crate) const ICR_INIT: i32 = 0x00000500; // Delivery Mode: INIT
    pub(crate) const ICR_STARTUP: i32 = 0x00000600; // Deliverymode: Start Up
    pub(crate) const ICR_DELIVS: i32 = 0x00001000; // Delivery Status: Send Pending if set, otherwise Idle.
    pub(crate) const ICR_ASSERT: i32 = 0x00004000; // Level: Assert interrupt if set, otherwise de-assert
    pub(crate) const ICR_LEVEL: i32 = 0x00008000; // Level: Assert if set, otherwise De-Assert.
    pub(crate) const ICR_BCAST: i32 = 0x00080000; // Destination: All Including Self

    pub(crate) const TDCR_X1: i32 = 0x0000000b; // divide counts by 1
}

struct LocalAPIC(VirtAddr);

impl LocalAPIC {
    fn write(&self, index: isize, value: i32) {
        unsafe {
            let p = self.as_mut_ptr();
            p.offset(index).write(value);
            p.offset(ID).read(); // wait for write to finish, by reading
        }
    }

    fn read(&self, index: isize) -> i32 {
        unsafe {
            let p = self.as_ptr();
            p.offset(index).read()
        }
    }

    /// See Intel SDM Vol.3 10.4.6 Local APIC ID
    fn cpu_num(&self) -> i32 {
        self.read(ID) >> 24
    }

    /// See Intel SDM Vol.3 10.4.8 Local APIC Version Register
    fn max_lvt_entry(&self) -> i32 {
        let v = self.read(VER);
        (v >> 16) & 0xff
    }

    /// Clear error status register (requires back-to-back writes).
    /// See Intel SDM Vol.3 10.5.3 Error Handling
    fn reset_esr(&self) {
        // First write is to clear register.
        self.write(ESR, 0);
        // According to the manual, local APIC might update the register
        // based on an error detected since the last write to the ESR.
        // It means one error might exist at most, so the second write
        // is required and enough to reset ESR?
        self.write(ESR, 0);
    }

    /// Acknowledge interrupt
    fn eoi(&self) {
        self.write(EOI, 0);
    }

    /// Spin for a given number of microseconds.
    /// On real hardware would want to tune this dynamically.
    fn micro_delay(&self, _us: u32) {}

    fn as_ptr(&self) -> *const i32 {
        self.0.as_ptr()
    }

    fn as_mut_ptr(&self) -> *mut i32 {
        self.0.as_mut_ptr()
    }
}

static mut LAPIC: Option<LocalAPIC> = None;

pub(crate) fn lapic_init() {
    let lapic_addr = mpconfig::lapic_addr().expect("lapic_addr not found");

    // lapicaddr is the physical address of the LAPIC's 4K MMIO
    // region.  Map it in to virtual memory so we can access it.
    let lapic = {
        let va = pmap::mmio_map_region(lapic_addr, PGSIZE as usize);
        unsafe { LAPIC = Some(LocalAPIC(va)) }
        unsafe { LAPIC.as_ref().unwrap() }
    };

    // Enable local APIC; set spurious interrupt vector.
    //
    // Set SVR_ENABLE of SVR is one way of enabling local APIC according to Intel SVM 10.4.3.
    // I'm not sure what spurious interrupt is, but it is something like unexpected interrupt?
    lapic.write(SVR, SVR_ENABLE | ((IRQ_OFFSET + IRQ_SPURIOUS) as i32));

    // The timer repeatedly counts down at bus frequency
    // from lapic[TICR] and then issues an interrupt.
    // If we cared more about precise timekeeping,
    // TICR would be calibrated using an external time source.
    //
    // See Intel SDM Vol3 10.5.4 APIC Timer
    lapic.write(TDCR, TDCR_X1);
    lapic.write(
        LVT_TIMER,
        LVT_TIMER_PERIODIC | (IRQ_OFFSET + IRQ_TIMER) as i32,
    );
    lapic.write(TICR, 10000000);

    // Leave LINT0 of the BSP enabled so that it can get
    // interrupts from the 8259A chip.
    //
    // According to Intel MP Specification,
    // the BIOS should initialize BSP's local APIC in Virtual Wire Mode (3.6.2.1 PIC Mode),
    // in which 8259A's INTR is virtually connected to BSP's LINTIN0.
    //
    // In this mode, we do not need to program the IOAPIC.
    if mpconfig::this_cpu().cpu_id != mpconfig::boot_cpu().cpu_id {
        lapic.write(LVT_LINT0, LVT_MASKED);
    }

    // Disable NMI (LINT1) on all CPUs
    lapic.write(LVT_LINT1, LVT_MASKED);

    // Disable performance counter overflow interrupts
    // on machines that provide that interrupt entry.
    //
    // According to Intel SDM Vol.3 10.4.8 Local APIC Version Register,
    // the value returned is 4 for the P6 family processors (which have 5 LVT entries).
    if lapic.max_lvt_entry() >= 4 {
        lapic.write(LVT_PC, LVT_MASKED);
    }

    // Map error interrupt to IRQ_ERROR.
    lapic.write(LVT_ERROR, (IRQ_OFFSET + IRQ_ERROR) as i32);

    // Clear error status register (requires back-to-back writes).
    lapic.reset_esr();

    // Ack any outstanding interrupts.
    lapic.eoi();

    // Send an Init Level De-Assert to synchronize arbitration ID's.
    //
    // ICR allows software running on the processor to specify and send
    // interprocessor interrupts (IPIs) to other processors in the system.
    // Here perform INIT Level De-assert to set arbitration IDs of each
    // processor to the values of their APIC IDs.
    //
    // "arbitration" is used to determine which processor an interrupt
    // is delivered to? (from Intel SDM Vol.3 10.7 System and APIC Bus Arbitration)
    //
    // See Intel SDM Vol.3 10.6.1 Interrupt Command Register (ICR)
    lapic.write(ICRHI, 0);
    lapic.write(ICRLO, ICR_BCAST | ICR_INIT | ICR_LEVEL);
    while lapic.read(ICRLO) & ICR_DELIVS != 0 {}

    // Enable interrupts on the APIC (but not on the processor).
    // See Intel SDM Vol.3 10.8.3.1 Task and Processor Priorities
    lapic.write(TPR, 0);
}

/// Start additional processor running entry code at addr.
/// See Appendix B of MultiProcessor Specification.
///
/// addr must be in form of 0x000VV000.
pub(crate) fn startap(apic_id: u8, addr: PhysAddr) {
    assert!(((addr.0 & 0xfff) == 0) && ((addr.0 >> 20) == 0) && addr.0 != 0);

    let lapic = unsafe { LAPIC.as_ref().expect("LAPIC should exist") };

    // "The BSP must initialize CMOS shutdown code to 0AH
    // and the warm reset vector (DWORD based at 40:67) to point at
    // the AP startup code prior to the [universal startup algorithm]."
    //
    // See for values http://www.bioscentral.com/misc/cmosmap.htm
    kclock::mc146818_write(0x0f, 0x0a); // reset PC without power off (restart from POST, and then execute codes in reset vector in real mode)
    {
        let pa = PhysAddr(0x40 << 4 | 0x67).to_va(); // Warm reset vector
        let p = pa.as_mut_ptr::<u16>();
        unsafe { p.offset(0).write(0) }; // offset?
        unsafe { p.offset(1).write((addr.0 >> 4) as u16) }; // segment? maybe see in real mode.
    }

    // "Universal startup algorithm."
    // Send INIT (level-triggered) interrupt to reset other CPU.
    lapic.write(ICRHI, (apic_id as i32) << 24);
    lapic.write(ICRLO, ICR_INIT | ICR_LEVEL | ICR_ASSERT);
    lapic.micro_delay(200);
    lapic.write(ICRLO, ICR_INIT | ICR_LEVEL);
    lapic.micro_delay(100); // should be 10ms, but too slow in Bochs!

    // Send startup IPI (twice!) to enter code.
    // Regular hardware is supposed to only accept a STARTUP
    // when it is in the halted state due to an INIT.  So the second
    // should be ignored, but it is part of the official Intel algorithm.
    // Bochs complains about the second one.  Too bad for Bochs.
    //
    // This causes the target processor to start executing in Real Mode from address
    // 000VV000h, where VV is an 8-bit vector that is part of the IPI message.
    //
    // See in B.4.2.
    for _ in 0..2 {
        lapic.write(ICRHI, (apic_id as i32) << 24);
        lapic.write(ICRLO, ICR_STARTUP | ((addr.0 as i32) >> 12));
        lapic.micro_delay(200);
    }
}

pub(crate) fn cpu_num() -> i32 {
    unsafe { LAPIC.as_ref().map(|lapic| lapic.cpu_num()).unwrap_or(0) }
}
