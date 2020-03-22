// Simple PIO-based (non-DMA) IDE driver code.
// ref. [OSDev](https://wiki.osdev.org/PCI_IDE_Controller)
// ref. [Spec](http://www.t13.org/Documents/UploadedDocuments/project/d0791r4c-ATA-1.pdf)
// ref. [About Compatibility Mode](http://www.bswd.com/pciide.pdf)

use crate::buf::consts::{BUF_FLAGS_DIRTY, BUF_FLAGS_VALID};
use crate::buf::Buf;
use crate::constants::*;
use crate::pmap::VirtAddr;
use crate::spinlock::Mutex;
use crate::trap::consts::IRQ_IDE;
use crate::{picirq, util, x86};
use consts::*;
use core::ptr::null_mut;

mod consts {
    // status
    pub(crate) const SR_BSY: u8 = 0x80; // busy
    pub(crate) const SR_DRDY: u8 = 0x40; // drive ready
    pub(crate) const SR_DWF: u8 = 0x20; // drive write fault
    pub(crate) const SR_ERR: u8 = 0x01; // error

    pub(crate) const PRIMARY_COMMAND_BASE_REG: u16 = 0x1f0; // for sending command to drive or posting status from the drive
    pub(crate) const PRIMARY_CONTROL_BASE_REG: u16 = 0x3f6; // for drive control and post alternate status

    // register
    // `PRIMARY_BASE_REG + reg` is a target port
    pub(crate) const REG_DATA: u16 = 0x00; // Read-Write
    pub(crate) const REG_ERROR: u16 = 0x01; // Read Only
    pub(crate) const REG_FEATURES: u16 = 0x01; // Write Only
    pub(crate) const REG_SECCOUNT0: u16 = 0x02; // Read-Write
    pub(crate) const REG_LBA0: u16 = 0x03; // Read-Write
    pub(crate) const REG_LBA1: u16 = 0x04; // Read-Write
    pub(crate) const REG_LBA2: u16 = 0x05; // Read-Write
    pub(crate) const REG_HDDEVSEL: u16 = 0x6; // Read-Write, used to select a drive in the channel.
    pub(crate) const REG_COMMAND: u16 = 0x07; // Write Only
    pub(crate) const REG_STATUS: u16 = 0x07; // Read Only

    // Command codes
    // See 9 Command Description in Spec
    pub(crate) const IDE_CMD_READ: u8 = 0x20;
    pub(crate) const IDE_CMD_WRITE: u8 = 0x30;
    pub(crate) const IDE_CMD_RDMUL: u8 = 0xc4;
    pub(crate) const IDE_CMD_WRMUL: u8 = 0xc5;
}

struct BufQueue {
    head: *mut Buf,
    len: usize,
}

unsafe impl Sync for BufQueue {}
unsafe impl Send for BufQueue {}

impl BufQueue {
    const fn new() -> BufQueue {
        BufQueue {
            head: null_mut(),
            len: 0,
        }
    }

    fn size(&self) -> usize {
        self.len
    }

    fn push(&mut self, b: *mut Buf) {
        unsafe {
            if self.head.is_null() {
                self.head = b;
            } else {
                let mut prev = self.head;
                while !(*prev).qnext.is_null() {
                    prev = (*prev).qnext;
                }
                (*prev).qnext = b;
            }
            self.len += 1;
        }
    }

    fn pop(&mut self) -> Option<*mut Buf> {
        unsafe {
            if self.head.is_null() {
                None
            } else {
                let res = self.head;
                self.head = (*self.head).qnext;
                self.len -= 1;
                Some(res)
            }
        }
    }
}

static BUF_QUEUE: Mutex<BufQueue> = Mutex::new(BufQueue::new());

/// Wait until disk to be ready.
fn ide_wait_ready(check_error: bool) -> bool {
    let mut r: u8;

    loop {
        // ref. 7.2.13 Status register in Spec
        r = x86::inb(PRIMARY_COMMAND_BASE_REG + REG_STATUS);
        if (r & (SR_BSY | SR_DRDY)) == SR_DRDY {
            break;
        }
    }

    !check_error || ((r & (SR_DWF | SR_ERR)) == 0)
}

/// Check whether Device 1 exists.
/// (With qemu, it means that we have an option like `-drive file=fs.img,index=1,media=disk,format=raw`)
fn ide_probe_disk1() -> bool {
    // wait for Device 0 to be ready
    if !ide_wait_ready(true) {
        panic!("something wrong with ide");
    }

    // switch to Device 1
    // ref. 7.2.8 Drive/head register in Spec
    x86::outb(PRIMARY_COMMAND_BASE_REG + REG_HDDEVSEL, 0xe0 | (1 << 4));

    // check whether Device 1 exists and get ready
    let mut found: bool = false;
    for _ in 0..1000 {
        let r = x86::inb(PRIMARY_COMMAND_BASE_REG + REG_STATUS);
        if r != 0 {
            if r & (SR_BSY | SR_DWF | SR_ERR) == 0 {
                found = true;
                break;
            }
        }
    }

    // switch back to Device 0
    x86::outb(PRIMARY_COMMAND_BASE_REG + REG_HDDEVSEL, 0xe0 | (0 << 4));

    print!("Device 1 presence: ");
    if found {
        println!("yes");
    } else {
        println!("no");
    }
    found
}

fn ide_start(b: &Buf) {
    if b.blockno >= (FS_SIZE as u32) {
        panic!("ide_start: incorrect blockno");
    }

    let sector_per_block = (BLK_SIZE / SECTOR_SIZE) as u32;
    let sector = b.blockno * sector_per_block;
    let read_cmd = if sector_per_block == 1 {
        IDE_CMD_READ
    } else {
        IDE_CMD_RDMUL
    };
    let write_cmd = if sector_per_block == 1 {
        IDE_CMD_WRITE
    } else {
        IDE_CMD_WRMUL
    };

    if sector_per_block > 7 {
        panic!("ide_start: illegal sector per block");
    }

    if !ide_wait_ready(true) {
        panic!("ide_start: something bad occurred.")
    }

    // This is Device Control Register? (7.2.6 in Spec).
    // This enables to generate interrupt?
    // This exists only in xv6 not in JOS, so may be not mandatory.
    x86::outb(PRIMARY_CONTROL_BASE_REG, 0); // generate interrupt

    // This register contains the number of sectors of data requested to be transferred
    // on a read or write operation between the host and the drive.
    // See 7.2 in Spec.
    x86::outb(
        PRIMARY_COMMAND_BASE_REG + REG_SECCOUNT0,
        sector_per_block as u8,
    ); // number of sectors

    // This register contains the starting sector number for any disk data access
    // for the subsequent command.
    // As we set up in `ide_probe_disk1`, addressing is based on LBA not CHS.
    // See 7.2 in Spec.
    x86::outb(PRIMARY_COMMAND_BASE_REG + REG_LBA0, (sector & 0xff) as u8);
    x86::outb(
        PRIMARY_COMMAND_BASE_REG + REG_LBA1,
        ((sector >> 8) & 0xff) as u8,
    );
    x86::outb(
        PRIMARY_COMMAND_BASE_REG + REG_LBA2,
        ((sector >> 16) & 0xff) as u8,
    );
    x86::outb(
        PRIMARY_COMMAND_BASE_REG + REG_HDDEVSEL,
        0xe0 | (((b.dev & 1) as u8) << 4) | (((sector >> 24) & 0x0f) as u8),
    );

    if b.flags & BUF_FLAGS_DIRTY != 0 {
        // This register contains the command code being sent to the drive.
        // Command execution begins immediately after this register is written.
        //
        // The detail of write protocol is in 10.2 of Spec
        x86::outb(PRIMARY_COMMAND_BASE_REG + REG_COMMAND, write_cmd);
        x86::outsl(
            PRIMARY_COMMAND_BASE_REG + REG_DATA,
            b.data.as_ptr().cast::<u32>(),
            BLK_SIZE / 4,
        );
    } else {
        // The detail of read protocol is in 10.1 of Spec
        x86::outb(PRIMARY_COMMAND_BASE_REG + REG_COMMAND, read_cmd);
    }
}

/// Interrupt handler.
pub(crate) fn ide_intr() {
    // The first queued buffer is the active request.
    let mut queue = BUF_QUEUE.lock();

    if let Some(b) = queue.pop() {
        let b = unsafe { &mut *b };

        if !ide_wait_ready(true) {
            panic!("ide_intr: something bad occurred.");
        }

        // Read data if needed.
        if b.flags & BUF_FLAGS_DIRTY == 0 {
            x86::insl(
                PRIMARY_COMMAND_BASE_REG + REG_DATA,
                b.data.as_mut_ptr().cast::<u32>(),
                BLK_SIZE / 4,
            );
        }

        b.flags |= BUF_FLAGS_VALID;
        b.flags &= !BUF_FLAGS_DIRTY;

        // Wake process waiting for this buf.
        // wakeup(b);

        // Stat disk on next buf in queue
        if let Some(next_b) = queue.pop() {
            let next_b = unsafe { &mut *next_b };
            ide_start(next_b);
        }
    }
}

/// Sync buf with disk.
/// If B_DIRTY is set, write buf to disk, clear B_DIRTY, set B_VALID.
/// Else if B_VALID is not set, read buf from disk, set B_VALID.
pub(crate) fn ide_rw(b: &mut Buf) {
    if (b.flags & (BUF_FLAGS_VALID | BUF_FLAGS_DIRTY)) == BUF_FLAGS_VALID {
        panic!("ide_rw: nothing to do");
    }

    {
        let mut queue = BUF_QUEUE.lock();

        // Append b to queue
        b.qnext = null_mut();
        queue.push(b);

        // There is only one Buf, which is one just appended.
        if queue.size() == 1 {
            ide_start(b);
        }
    }

    // Wait for request to finish.
    while (b.flags & (BUF_FLAGS_VALID | BUF_FLAGS_DIRTY)) != BUF_FLAGS_VALID {
        x86::sti();
        // sleep(b, &idelock);
        x86::cli();
    }
}

pub(crate) fn ide_init() {
    if !ide_probe_disk1() {
        panic!("Device 1 must be available");
    }

    picirq::unmask_8259a(IRQ_IDE);
}
