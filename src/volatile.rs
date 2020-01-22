// This comes from [volatile](https://crates.io/crates/volatile), MIT license

use core::ptr;

#[derive(Debug)]
#[repr(transparent)]
pub struct Volatile<T: Copy>(T);

impl<T: Copy> Volatile<T> {
    #[allow(dead_code)]
    pub const fn new(value: T) -> Volatile<T> {
        Volatile(value)
    }

    pub fn read(&self) -> T {
        unsafe { ptr::read_volatile(&self.0) }
    }

    pub fn write(&mut self, value: T) {
        unsafe { ptr::write_volatile(&mut self.0, value) }
    }
}

impl<T: Copy> Clone for Volatile<T> {
    fn clone(&self) -> Self {
        Volatile(self.read())
    }
}
