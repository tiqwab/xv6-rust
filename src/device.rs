use crate::console;
use crate::constants::*;
use crate::fs::Inode;
use crate::once::Once;
use alloc::boxed::Box;

pub(crate) struct DevSw {
    /// Return None if device is not prepared for read.
    pub(crate) read: Box<dyn Fn(&Inode, *mut u8, usize) -> Option<i32>>,
    pub(crate) write: Box<dyn Fn(&Inode, *const u8, usize) -> i32>,
}

fn do_nothing_read(_inode: &Inode, _buf: *mut u8, _count: usize) -> Option<i32> {
    Some(0)
}

fn do_nothing_write(_inode: &Inode, _buf: *const u8, _count: usize) -> i32 {
    0
}

unsafe impl Sync for DevSw {}
unsafe impl Send for DevSw {}

static DEV_SW: Once<[Option<DevSw>; NDEV]> = Once::new();

pub(crate) fn get_dev_sw(idx: usize) -> Option<&'static DevSw> {
    let dev_sw = DEV_SW.call_once(|| {
        let mut res = [None; NDEV];

        res[CONSOLE] = Some(DevSw {
            read: Box::new(console::console_read),
            write: Box::new(console::console_write),
        });

        res
    });

    dev_sw.get(idx).and_then(|sw_opt| sw_opt.as_ref())
}
