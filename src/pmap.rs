use core::ops::{Add, Index, IndexMut, Sub};

use crate::constants::*;
use crate::kclock;
use core::mem;
use core::ptr::null_mut;

extern "C" {
    static end: u32;
}

#[derive(Debug, Clone, Copy)]
struct VirtualAddr(u32);

impl VirtualAddr {
    /// VirtualAddr in kernel can be converted into PhysAddr.
    fn to_pa(&self) -> PhysAddr {
        if self.0 < KERN_BASE {
            panic!(
                "cannot convert virtual address 0x{:x} to physical address",
                self.0
            );
        }
        PhysAddr(self.0 - KERN_BASE)
    }
}

impl Add<u32> for VirtualAddr {
    type Output = VirtualAddr;

    fn add(self, rhs: u32) -> Self::Output {
        VirtualAddr(self.0 + rhs)
    }
}

impl Sub<u32> for VirtualAddr {
    type Output = VirtualAddr;

    fn sub(self, rhs: u32) -> Self::Output {
        VirtualAddr(self.0 - rhs)
    }
}

#[derive(Debug, Clone, Copy)]
struct PhysAddr(u32);

struct BootAllocator {
    bss_end: VirtualAddr,
    next_free: Option<VirtualAddr>,
}

impl BootAllocator {
    pub fn new(bss_end: VirtualAddr) -> BootAllocator {
        BootAllocator {
            bss_end: bss_end,
            next_free: None,
        }
    }

    /// This simple physical memory allocator is used only while JOS is setting
    /// up its virtual memory system.  page_alloc() is the real allocator.
    ///
    /// If n>0, allocates enough pages of contiguous physical memory to hold 'n'
    /// bytes.  Doesn't initialize the memory.  Returns a kernel virtual address.
    ///
    /// If n==0, returns the address of the next free page without allocating
    /// anything.
    ///
    /// If we're out of memory, boot_alloc should panic.
    /// This function may ONLY be used during initialization,
    /// before the page_free_list list has been set up.
    fn alloc(&mut self, n: u32) -> VirtualAddr {
        match self.next_free.take() {
            None => {
                let next = round_up_va(self.bss_end, PGSIZE);
                self.next_free = Some(round_up_va(next + n, PGSIZE));
                next
            }
            Some(next) => {
                self.next_free = Some(round_up_va(next + n, PGSIZE));
                next
            }
        }
    }
}

#[repr(align(4096))]
#[repr(C)]
struct PageDirectory {
    entries: [PDE; NPDENTRIES],
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct PDX(VirtualAddr);

impl Index<usize> for PageDirectory {
    type Output = PDE;

    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index]
    }
}

impl IndexMut<usize> for PageDirectory {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.entries[index]
    }
}

impl Index<PDX> for PageDirectory {
    type Output = PDE;

    fn index(&self, index: PDX) -> &Self::Output {
        let addr = (index.0).0 as usize;
        let addr = (addr >> 22) & 0x3ff;
        &self[addr]
    }
}

impl IndexMut<PDX> for PageDirectory {
    fn index_mut(&mut self, index: PDX) -> &mut Self::Output {
        let addr = (index.0).0 as usize;
        let addr = (addr >> 22) & 0x3ff;
        &mut self[addr]
    }
}

#[repr(C)]
struct PDE(u32);

impl PDE {
    fn new(pa: PhysAddr, attr: u32) -> PDE {
        PDE(pa.0 | attr)
    }
}

fn round_up_u32(x: u32, base: u32) -> u32 {
    ((x - 1 + base) / base) * base
}

fn round_up_va(x: VirtualAddr, base: u32) -> VirtualAddr {
    VirtualAddr(round_up_u32(x.0, base))
}

fn nvram_read(reg: u8) -> u16 {
    let low = kclock::mc146818_read(reg) as u16;
    let high = kclock::mc146818_read(reg + 1) as u16;
    low | (high << 8)
}

/// Return (npages, npages_basemem).
/// npages: the amount of physical memory (in pages).
/// napages_basemem: the amount of base memory (in pages).
fn i386_detect_memory() -> (u32, u32) {
    // Use CMOS calls to measure available base & extended memory.
    // (CMOS calls return results in kilobytes.)
    let basemem = nvram_read(kclock::NVRAM_BASELO) as u32;
    let extmem = nvram_read(kclock::NVRAM_EXTLO) as u32;
    let ext16mem = (nvram_read(kclock::NVRAM_EXT16LO) as u32) * 64;

    let totalmem = if ext16mem > 0 {
        16 * 1024 + ext16mem
    } else if extmem > 0 {
        1 * 1024 + extmem
    } else {
        basemem
    };

    let npages = totalmem / (PGSIZE / 1024);
    let npages_basemem = basemem / (PGSIZE / 1024);

    println!(
        "Physical memory: {}KB available, base = {}K, extended = {}K",
        totalmem,
        basemem,
        totalmem - basemem
    );
    println!("npages: {}, npages_baseme: {}", npages, npages_basemem);

    (npages, npages_basemem)
}

pub fn mem_init() {
    // Find out how much memory the machine has (npages & npages_basemem).
    let (npages, npages_basemem) = i386_detect_memory();

    // create initial page directory.
    let bss_end = VirtualAddr(unsafe { &end as *const _ as u32 });
    let mut boot_allocator = BootAllocator::new(bss_end);
    let kern_pgdir_va = boot_allocator.alloc(PGSIZE);
    println!("kern_pgdir: 0x{:x}", kern_pgdir_va.0);
    // memset(kern_pgdir, 0, PGSIZE);

    // Recursively insert PD in itself as a page table, to form
    // a virtual page table at virtual address UVPT.
    // Permissions: kernel R, user R
    let kern_pgdir = unsafe { &mut *(kern_pgdir_va.0 as *mut PageDirectory) };
    let uvpt = VirtualAddr(UVPT);
    let entry = PDE::new(kern_pgdir_va.to_pa(), PTE_P | PTE_U);
    let index = PDX(uvpt);
    kern_pgdir[index] = entry;
    println!(
        "&kern_pgdir[PDX(uvpt)](0x{:?}): 0x{:x}",
        &kern_pgdir[index] as *const PDE, kern_pgdir[index].0
    );

    // Allocate an array of npages 'struct PageInfo's and store it in 'pages'.
    // The kernel uses this array to keep track of physical pages: for
    // each physical page, there is a corresponding struct PageInfo in this
    // array.  'npages' is the number of physical pages in memory.  Use memset
    // to initialize all fields of each struct PageInfo to 0.
    let page_info_size = mem::size_of::<PageInfo>();
    let pages = boot_allocator.alloc(npages * page_info_size as u32).0 as *mut PageInfo;
    let mut allocator = PageAllocator {
        page_free_list: null_mut(),
        pages: pages,
    };
    // memset(pages, 0, npages * sizeof (struct PageInfo));
    println!("pages: 0x{:?}", pages);

    // Now that we've allocated the initial kernel data structures, we set
    // up the list of free physical pages. Once we've done so, all further
    // memory management will go through the page_* functions. In
    // particular, we can now map memory using boot_map_region
    // or page_insert
    allocator.init(&mut boot_allocator, npages, npages_basemem);
}

// --------------------------------------------------------------
// Tracking of physical pages.
// The 'pages' array has one 'struct PageInfo' entry per physical page.
// Pages are reference counted, and free pages are kept on a linked list.
// --------------------------------------------------------------

#[repr(C)]
struct PageInfo {
    pp_link: *mut PageInfo,
    pp_ref: u16,
}

struct PageAllocator {
    page_free_list: *mut PageInfo,
    pages: *mut PageInfo,
}

impl PageAllocator {
    /// Initialize page structure and memory free list.
    /// After this is done, NEVER use boot_alloc again.  ONLY use the page
    /// allocator functions below to allocate and deallocate physical
    /// memory via the page_free_list.
    fn init(&mut self, ba: &mut BootAllocator, npages: u32, npages_basemem: u32) {
        let first_free_page = ba.alloc(0).to_pa().0 / PGSIZE;
        let mut prev: *mut PageInfo = null_mut();
        for i in 0..npages {
            if i >= npages_basemem && i < first_free_page {
                println!("{}", i);
                continue;
            }
            let page = unsafe { &mut *(self.pages.add(i as usize)) };
            page.pp_ref = 0;
            page.pp_link = prev;
            self.page_free_list = page as *mut PageInfo;
        }
    }
}
