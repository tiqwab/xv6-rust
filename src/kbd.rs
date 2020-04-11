// ref. https://wiki.osdev.org/PS/2_Keyboard
// ref. https://wiki.osdev.org/%228042%22_PS/2_Controller
// ref. http://oswiki.osask.jp/?cmd=read&page=(AT)keyboard

use crate::kbd::consts::*;
use crate::trap::consts::IRQ_KBD;
use crate::{picirq, x86};

mod consts {
    // I/O port
    pub(crate) const PORT_DATA: u16 = 0x60; // kbd data port (I)
    pub(crate) const PORT_STATUS: u16 = 0x64; // kbd controller status port (I) when read. command port when write.

    // flags for status register
    pub(crate) const STATUS_FL_DIB: u8 = 0x01; // kbd data in buffer. must be set before attempting to read data from IO port 0x60)
}

extern "C" {
    static normal_map: [u8; 256];
}

pub(crate) fn kbd_getc() -> Option<u8> {
    unsafe {
        let st = x86::inb(PORT_STATUS);
        if (st & STATUS_FL_DIB) == 0 {
            return None;
        }

        let data = x86::inb(PORT_DATA);
        println!(
            "kbd raw data: {:x}, as ascii: {}",
            data, normal_map[data as usize] as char
        );

        Some(data)
    }
}

pub(crate) fn kbd_init() {
    picirq::unmask_8259a(IRQ_KBD);
}
