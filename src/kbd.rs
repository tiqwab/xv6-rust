// ref. https://wiki.osdev.org/PS/2_Keyboard
// ref. https://wiki.osdev.org/%228042%22_PS/2_Controller
// ref. http://oswiki.osask.jp/?cmd=read&page=(AT)keyboard

use crate::kbd::consts::*;
use crate::trap::consts::IRQ_KBD;
use crate::{picirq, x86};
use core::ptr::null;

mod consts {
    // I/O port
    pub(crate) const PORT_DATA: u16 = 0x60; // kbd data port (I)
    pub(crate) const PORT_STATUS: u16 = 0x64; // kbd controller status port (I) when read. command port when write.

    // flags for status register
    pub(crate) const STATUS_FL_DIB: u8 = 0x01; // kbd data in buffer. must be set before attempting to read data from IO port 0x60)

    // flags of shift
    pub(crate) const SHIFT_FL_SHIFT: u8 = 1 << 0;
    pub(crate) const SHIFT_FL_CTL: u8 = 1 << 1;
    pub(crate) const SHIFT_FL_ALT: u8 = 1 << 2;
    pub(crate) const SHIFT_FL_CAPSLOCK: u8 = 1 << 3;
    pub(crate) const SHIFT_FL_NUMLOCK: u8 = 1 << 4;
    pub(crate) const SHIFT_FL_SCROLLLOCK: u8 = 1 << 5;
    pub(crate) const SHIFT_FL_E0ESC: u8 = 1 << 6; // some keys have 0xe0 escape (e.g. cursor right)

    pub(crate) const KEY_RELEASED: u8 = 1 << 7; // input data at this bit is on when key is released.
}

extern "C" {
    static shift_code: [u8; 256];
    static toggle_code: [u8; 256];
    static normal_map: [u8; 256];
    static shift_map: [u8; 256];
    static ctl_map: [u8; 256];
}

pub(crate) fn kbd_getc() -> Option<u8> {
    unsafe {
        static mut shift: u8 = 0;
        let char_code: [&[u8; 256]; 4] = [&normal_map, &shift_map, &ctl_map, &ctl_map];

        let st = x86::inb(PORT_STATUS);
        if (st & STATUS_FL_DIB) == 0 {
            return None;
        }

        let mut data = x86::inb(PORT_DATA);

        if data == 0xe0 {
            shift |= SHIFT_FL_E0ESC;
            return None;
        } else if data & 0x80 != 0 {
            // Key released
            data = if shift & SHIFT_FL_E0ESC != 0 {
                data
            } else {
                data & !KEY_RELEASED
            };
            shift &= !(shift_code[data as usize] | SHIFT_FL_E0ESC);
            return None;
        } else if shift & SHIFT_FL_E0ESC != 0 {
            // Last character was an E0 escape; or with 0x80
            data |= KEY_RELEASED; // what is it? -> hack not to emit the valid key code?
            shift &= !SHIFT_FL_E0ESC;
        }

        shift |= shift_code[data as usize];
        shift ^= toggle_code[data as usize];

        let mut c = char_code[(shift & (SHIFT_FL_CTL | SHIFT_FL_SHIFT)) as usize][data as usize];

        if shift & SHIFT_FL_CAPSLOCK != 0 {
            let _c = c as char;
            if 'a' <= _c && _c <= 'z' {
                c -= 'a' as u8 - 'A' as u8;
            } else if 'A' <= _c && _c <= 'Z' {
                c += 'a' as u8 - 'A' as u8;
            }
        }

        println!("kbd raw data: 0x{:02x}, as ascii: {}", data, c as char);
        Some(c)
    }
}

pub(crate) fn kbd_init() {
    picirq::unmask_8259a(IRQ_KBD);
}
