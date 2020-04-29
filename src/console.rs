use crate::fs::Inode;
use crate::spinlock::{Mutex, MutexGuard};
use crate::{kbd, serial, vga_buffer};
use core::fmt;
use core::ptr::slice_from_raw_parts;

static CONSOLE_LOCK: Mutex<()> = Mutex::new(());

pub fn print(args: fmt::Arguments) {
    let _lock = CONSOLE_LOCK.lock();
    vga_buffer::_print(args);
    serial::_print(args);
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::console::print(format_args!($($arg)*));
    }
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => {
        $crate::print!("{}\n", format_args!($($arg)*));
    }
}

pub(crate) fn console_write(_inode: &Inode, buf: *const u8, count: usize) -> i32 {
    let sli = unsafe { &*slice_from_raw_parts(buf, count) };
    match core::str::from_utf8(sli) {
        Err(_) => {
            println!("Error in console_write: failed to create str");
            -1
        }
        Ok(str) => {
            print!("{}", str);
            count as i32
        }
    }
}

const INPUT_BUF: usize = 128;

struct Input {
    buf: [u8; INPUT_BUF],
    r: usize, // read index
    w: usize, // write index
    e: usize, // edit index
}

impl Input {
    const fn new() -> Input {
        Input {
            buf: [0; INPUT_BUF],
            r: 0,
            w: 0,
            e: 0,
        }
    }
}

static INPUT: Mutex<Input> = Mutex::new(Input::new());

fn get_input() -> MutexGuard<'static, Input> {
    INPUT.lock()
}

pub(crate) fn console_intr() {
    match kbd::kbd_getc() {
        None => {
            // do nothing
        }
        Some(c) => {
            let mut input = get_input();
            let orig_e = input.e;

            {
                if c == '\n' as u8 || input.e == input.r + INPUT_BUF {
                    print!("{}", c as char);
                    input.buf[orig_e as usize % INPUT_BUF] = c;
                    input.e = orig_e + 1;
                    input.w = input.e;
                } else if c == 0x08 {
                    // backspace
                    if input.e != input.w {
                        input.e = orig_e - 1;
                        serial::serial().put_bs();
                        vga_buffer::writer().write_bs();
                    }
                } else {
                    print!("{}", c as char);
                    input.buf[orig_e as usize % INPUT_BUF] = c;
                    input.e = orig_e + 1;
                }
            }
        }
    }
}

/// Return byte count read.
/// The function does not block.
pub(crate) fn console_read(_inode: &Inode, mut buf: *mut u8, n: usize) -> Option<i32> {
    let mut input = get_input();

    if input.r == input.w {
        return None;
    }

    let mut count = 0;

    while count < n && input.r != input.w {
        let orig_r = input.r;
        let c = input.buf[orig_r % INPUT_BUF];
        unsafe {
            *buf = c;
            buf = buf.add(1);
        }
        count += 1;
        input.r += 1;

        if c as char == '\n' {
            break;
        }
    }

    Some(count as i32)
}
