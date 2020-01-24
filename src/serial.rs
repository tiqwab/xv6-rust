// ref. https://wiki.osdev.org/Serial_Ports
// ref. console.c in xv6

use crate::x86;
use core::fmt;
use core::fmt::{Error, Write};

// TODO: Make it to be thread-safe
// ref. https://os.phil-opp.com/vga-text-mode/#spinlocks
static mut SERIAL: Option<Serial> = None;

struct Serial {
    #[allow(dead_code)]
    serial_exists: bool,
}

pub(crate) fn init_serial() {
    // Turn off the FIFO
    x86::outb(COM1 + COM_FCR, 0);

    // Set speed; requires DLAB latch
    x86::outb(COM1 + COM_LCR, COM_LCR_DLAB);
    x86::outb(COM1 + COM_DLL, 12); // 115200 / 9600
    x86::outb(COM1 + COM_DLM, 0);

    // 8 data bits, 1 stop bit, parity off; trun off DLAB latch
    x86::outb(COM1 + COM_LCR, COM_LCR_WLEN8 & (!COM_LCR_DLAB));

    // No modem controls
    x86::outb(COM1 + COM_MCR, 0);

    // Enable rcv interrupts
    x86::outb(COM1 + COM_IER, COM_IER_RDI);

    // Clear any preexisting overrun indications and interrupts
    // Serial port doesn't exist if COM_LSR returns 0xFF
    let serial_exists = x86::inb(COM1 + COM_LSR) != 0xFF;
    x86::inb(COM1 + COM_IIR);
    x86::inb(COM1 + COM_RX);

    unsafe {
        SERIAL = Some(Serial { serial_exists });
    }
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    unsafe {
        SERIAL.as_mut().unwrap().write_fmt(args).unwrap();
    }
}

#[allow(dead_code)]
const COM1: u16 = 0x3F8;

#[allow(dead_code)]
const COM_RX: u16 = 0; // In: Receive buffer (DLAB=0)
#[allow(dead_code)]
const COM_TX: u16 = 0; // Out: Transmit buffer (DLAB=0)
#[allow(dead_code)]
const COM_DLL: u16 = 0; // Out: Divisor Latch Low (DLAB=1)
#[allow(dead_code)]
const COM_DLM: u16 = 1; // Out: Divisor Latch High (DLAB=1)
#[allow(dead_code)]
const COM_IER: u16 = 1; // Out: Interrupt Enable Register
#[allow(dead_code)]
const COM_IER_RDI: u8 = 0x01; // Enable receiver data interrupt
#[allow(dead_code)]
const COM_IIR: u16 = 2; // In: Interrupt ID Register
#[allow(dead_code)]
const COM_FCR: u16 = 2; // Out: FIFO Control Register
#[allow(dead_code)]
const COM_LCR: u16 = 3; // Out: Line Control Register
#[allow(dead_code)]
const COM_LCR_DLAB: u8 = 0x80; // Divisor latch access bit
#[allow(dead_code)]
const COM_LCR_WLEN8: u8 = 0x03; // Wordlength: 8 bits
#[allow(dead_code)]
const COM_MCR: u16 = 4; // Out: Modem Control Register
#[allow(dead_code)]
const COM_MCR_RTS: u8 = 0x02; // RTS complement
#[allow(dead_code)]
const COM_MCR_DTR: u8 = 0x01; // DTR complement
#[allow(dead_code)]
const COM_MCR_OUT2: u8 = 0x08; // Out2 complement
#[allow(dead_code)]
const COM_LSR: u16 = 5; // In: Line Status Register
#[allow(dead_code)]
const COM_LSR_DATA: u8 = 0x01; // Data available
#[allow(dead_code)]
const COM_LSR_TXRDY: u8 = 0x20; // Transmit buffer avail
#[allow(dead_code)]
const COM_LSR_TSRE: u8 = 0x40; // Transmitter off

impl Serial {
    // Stupid I/O delay routine necessitated by historical PC design flaws
    fn delay(&self) {
        x86::inb(0x84);
        x86::inb(0x84);
        x86::inb(0x84);
        x86::inb(0x84);
    }

    #[allow(dead_code)]
    fn serial_proc_data(&self) -> Option<u8> {
        if (x86::inb(COM1 + COM_LSR) & COM_LSR_DATA) == 0 {
            None
        } else {
            Some(x86::inb(COM1 + COM_RX))
        }
    }

    fn serial_putc(&self, c: u8) {
        for _ in 0..12800 {
            if (x86::inb(COM1 + COM_LSR) & COM_LSR_TXRDY) != 0 {
                break;
            }
            self.delay();
        }
        x86::outb(COM1 + COM_TX, c);
    }
}

impl fmt::Write for Serial {
    fn write_str(&mut self, s: &str) -> Result<(), Error> {
        for b in s.bytes() {
            self.serial_putc(b)
        }
        Ok(())
    }
}
