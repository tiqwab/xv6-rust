use crate::spinlock::Mutex;
use crate::{serial, vga_buffer};
use core::fmt;

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
