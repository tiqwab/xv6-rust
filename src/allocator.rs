// Some of codes come from https://github.com/redox-os/kernel/blob/master/src/allocator/linked_list.rs

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::NonNull;
use linked_list_allocator::Heap;

static mut HEAP: Option<Heap> = None;

pub struct HeapAllocator;

impl HeapAllocator {
    pub unsafe fn init(offset: usize, size: usize) {
        HEAP = Some(Heap::new(offset, size));
    }
}

unsafe impl GlobalAlloc for HeapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let heap = HEAP.as_mut().expect("HEAP is not initialized yet");
        match heap.allocate_first_fit(layout) {
            Err(alloc_err) => {
                panic!("allocation error: {:?}", alloc_err);
            }
            Ok(res) => {
                #[cfg(feature = "debug")]
                println!(
                    "HeapAllocator: allocated for {:?} at 0x{:?}",
                    layout,
                    res.as_ptr()
                );
                res.as_ptr()
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let heap = HEAP.as_mut().expect("HEAP is not initialized yet");
        heap.deallocate(NonNull::new_unchecked(ptr), layout);
        #[cfg(feature = "debug")]
        println!("HeapAllocator: released {:?}", ptr);
    }
}
