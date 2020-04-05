// ref. https://wiki.osdev.org/8259_PIC
// ref. [8259A doc](https://pdos.csail.mit.edu/6.828/2018/readings/hardware/8259A.pdf)

use crate::spinlock::{Mutex, MutexGuard};
use crate::trap::consts::IRQ_OFFSET;
use crate::x86;
use consts::*;
use core::sync::atomic::{AtomicBool, Ordering};

static DID_INIT: AtomicBool = AtomicBool::new(false);
static IRQ_MASK_8259A: Mutex<u16> = Mutex::new(0xffff & !((1 << IRQ_SLAVE) as u16));

mod consts {
    // I/O ports to communicate with 8259 PIC
    // IRQs 0-7 for Master, IRQs 8-15 for Slave
    pub(crate) const IO_MASTER_COMMAND: u16 = 0x20;
    pub(crate) const IO_MASTER_DATA: u16 = 0x21;
    pub(crate) const IO_SLAVE_COMMAND: u16 = 0xA0;
    pub(crate) const IO_SLAVE_DATA: u16 = 0xA1;

    // IRQ at which slave connects to master
    pub(crate) const IRQ_SLAVE: u8 = 2;
}

// See OSDev or INITIALIZATION COMMAND WORDS in 8259A doc.
pub(crate) fn pic_init() {
    DID_INIT.store(true, Ordering::Release);

    // Mask all interrupts
    // We use local APIC, so disable 8259A PIC (but settings are effective?).
    // TODO: We really have to mask it?
    x86::outb(IO_MASTER_DATA, 0xff);
    x86::outb(IO_SLAVE_DATA, 0xff);

    // Set up master (8259A-1)

    // ICW1:  0001g0hi
    //    g:  0 = edge triggering, 1 = level triggering
    //    h:  0 = cascaded PICs, 1 = master only
    //    i:  0 = no ICW4, 1 = ICW4 required
    x86::outb(IO_MASTER_COMMAND, 0x11);

    // ICW2: Vector offset
    // This sets up to make CPU use (IRQ_OFFSET + i)-th interrupt handler for IRQ-i.
    x86::outb(IO_MASTER_DATA, IRQ_OFFSET);

    // ICW3:  bit mask of IR lines connected to slave PICs (master PIC),
    //        3-bit No of IR line at which slave connects to master(slave PIC).
    x86::outb(IO_MASTER_DATA, 1 << IRQ_SLAVE);

    // ICW4:  000nbmap
    //    n:  1 = special fully nested mode
    //    b:  1 = buffered mode
    //    m:  0 = slave PIC, 1 = master PIC
    //	  (ignored when b is 0, as the master/slave role
    //	  can be hardwired).
    //    a:  1 = Automatic EOI mode (AEOI mode)
    //    p:  0 = MCS-80/85 mode, 1 = intel x86 mode
    x86::outb(IO_MASTER_DATA, 0x03);

    // Set up slave (8259A-2)

    x86::outb(IO_SLAVE_COMMAND, 0x11); // ICW1
    x86::outb(IO_SLAVE_DATA, IRQ_OFFSET + 8); // ICW2
    x86::outb(IO_SLAVE_DATA, IRQ_SLAVE); // ICW3

    // NB Automatic EOI mode doesn't tend to work on the slave.
    // Linux source code says it's "to be investigated".
    //
    // But it is required for IDE interrupt and it does work at least in QEMU.
    x86::outb(IO_SLAVE_DATA, 0x03); // ICW4

    // OCW3:  0ef01prs
    //   ef:  0x = NOP, 10 = clear specific mask, 11 = set specific mask
    //    p:  0 = no polling, 1 = polling mode
    //   rs:  0x = NOP, 10 = read IRR, 11 = read ISR
    //
    // This enables Special Mask Mode?
    x86::outb(IO_MASTER_COMMAND, 0x68); // clear specific mask
    x86::outb(IO_MASTER_COMMAND, 0x0a); // read IRR by default
    x86::outb(IO_SLAVE_COMMAND, 0x68);
    x86::outb(IO_SLAVE_COMMAND, 0x0a);

    let mask = IRQ_MASK_8259A.lock();
    if *mask != 0xffff {
        set_mask_8259a(*mask, mask);
    }
}

pub(crate) fn unmask_8259a(irq: u8) {
    let mask = IRQ_MASK_8259A.lock();
    let new_mask = *mask & (!(1 << (irq as u16)));
    set_mask_8259a(new_mask, mask);
}

fn set_mask_8259a(new_mask: u16, mut mask: MutexGuard<u16>) {
    *mask = new_mask;
    if !DID_INIT.load(Ordering::Acquire) {
        return;
    }
    x86::outb(IO_MASTER_DATA, new_mask as u8);
    x86::outb(IO_SLAVE_DATA, (new_mask >> 8) as u8);
    print!("enabled interrupts:");
    for i in 0..16 {
        if !new_mask & (1 << i) != 0 {
            print!(" {}", i);
        }
    }
    println!();
}
