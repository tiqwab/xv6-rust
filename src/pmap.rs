use core::mem;
use core::ops::{Add, Index, IndexMut, Sub};
use core::ptr::{null, null_mut};

use crate::constants::*;
use crate::kclock;
use crate::x86;

extern "C" {
    static end: u32;
    static bootstack: u32;
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct VirtAddr(pub(crate) u32);

impl VirtAddr {
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

    fn is_aligned(&self) -> bool {
        self.0 % PGSIZE == 0
    }
}

impl Add<u32> for VirtAddr {
    type Output = VirtAddr;

    fn add(self, rhs: u32) -> Self::Output {
        VirtAddr(self.0 + rhs)
    }
}

impl Add<usize> for VirtAddr {
    type Output = VirtAddr;

    fn add(self, rhs: usize) -> Self::Output {
        VirtAddr(self.0 + (rhs as u32))
    }
}

impl Sub<u32> for VirtAddr {
    type Output = VirtAddr;

    fn sub(self, rhs: u32) -> Self::Output {
        VirtAddr(self.0 - rhs)
    }
}

impl Sub<usize> for VirtAddr {
    type Output = VirtAddr;

    fn sub(self, rhs: usize) -> Self::Output {
        VirtAddr(self.0 - (rhs as u32))
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PhysAddr(pub(crate) u32);

impl PhysAddr {
    fn to_va(&self) -> VirtAddr {
        assert!(self.0 <= 0xf0000000, "PhysAddr(0x{:x}) is too high", self.0);
        VirtAddr(self.0 | KERN_BASE)
    }

    fn is_aligned(&self) -> bool {
        self.0 % PGSIZE == 0
    }
}

impl Add<u32> for PhysAddr {
    type Output = PhysAddr;

    fn add(self, rhs: u32) -> Self::Output {
        PhysAddr(self.0 + rhs)
    }
}

impl Add<usize> for PhysAddr {
    type Output = PhysAddr;

    fn add(self, rhs: usize) -> Self::Output {
        PhysAddr(self.0 + (rhs as u32))
    }
}

impl Sub<u32> for PhysAddr {
    type Output = PhysAddr;

    fn sub(self, rhs: u32) -> Self::Output {
        PhysAddr(self.0 - rhs)
    }
}

impl Sub<usize> for PhysAddr {
    type Output = PhysAddr;

    fn sub(self, rhs: usize) -> Self::Output {
        PhysAddr(self.0 - (rhs as u32))
    }
}

struct BootAllocator {
    bss_end: VirtAddr,
    next_free: Option<VirtAddr>,
}

impl BootAllocator {
    pub fn new(bss_end: VirtAddr) -> BootAllocator {
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
    fn alloc(&mut self, n: u32) -> VirtAddr {
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
pub(crate) struct PageDirectory {
    entries: [PDE; NPDENTRIES],
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct PDX(VirtAddr);

impl PageDirectory {
    pub(crate) const fn new() -> PageDirectory {
        PageDirectory {
            entries: [PDE::empty(); NPDENTRIES],
        }
    }

    fn get(&mut self, pdx: PDX) -> Option<&mut PDE> {
        if self[pdx].exists() {
            Some(&mut self[pdx])
        } else {
            None
        }
    }

    /// Given 'pgdir', a pointer to a page directory, pgdir_walk returns
    /// a pointer to the page table entry (PTE) for linear address 'va'.
    /// This requires walking the two-level page table structure.
    ///
    /// The relevant page table page might not exist yet.
    /// If this is true, and create == false, then pgdir_walk returns NULL.
    /// Otherwise, pgdir_walk allocates a new page table page with page_alloc.
    ///    - If the allocation fails, pgdir_walk returns NULL.
    ///    - Otherwise, the new page's reference count is incremented,
    ///	the page is cleared,
    ///	and pgdir_walk returns a pointer into the new page table page.
    fn walk(
        &mut self,
        va: VirtAddr,
        should_create: bool,
        allocator: &mut PageAllocator,
    ) -> Option<&mut PTE> {
        let pdx = PDX(va);
        let pde = &mut self[pdx];
        if !pde.exists() {
            if !should_create {
                return None;
            }
            let pa = allocator.alloc(AllocFlag::AllocZero).unwrap();
            pde.set(pa, PTE_U | PTE_P | PTE_W);
            allocator.incref_pde(pde);
        }

        let pt = pde.table();
        // println!("walk: pt for va({:x}): {:?}", va.0, pt as *mut PageTable);
        let ptx = PTX(va);
        Some(&mut pt[ptx])
    }

    /// Map [va, va+size) of virtual address space to physical [pa, pa+size)
    /// in the page table rooted at pgdir.  Size is a multiple of PGSIZE, and
    /// va and pa are both page-aligned.
    /// Use permission bits perm|PTE_P for the entries.
    fn boot_map_region(
        &mut self,
        start_va: VirtAddr,
        size: usize,
        start_pa: PhysAddr,
        perm: u32,
        allocator: &mut PageAllocator,
    ) {
        assert!(start_va.is_aligned(), "start_va is not page aligned.");
        assert!(start_pa.is_aligned(), "start_pa is not page aligned.");
        assert_eq!(
            size % (PGSIZE as usize),
            0,
            "size should be multiple of PGSIZE"
        );

        for i in 0..(size / (PGSIZE as usize)) {
            let va = start_va + i * (PGSIZE as usize);
            let pa = start_pa + i * (PGSIZE as usize);
            let pte = self.walk(va, true, allocator).unwrap();
            pte.set(pa, perm | PTE_P);
            // println!("va: 0x{:x}, pte: 0x{:x}", va.0, pte.0);
        }
    }

    // Return the page mapped at virtual address 'va'.
    // PTE is used by page_remove and
    // can be used to verify page permissions for syscall arguments,
    // but should not be used by most callers.
    //
    // Return None if there is no page mapped at va.
    fn lookup(&mut self, va: VirtAddr, allocator: &mut PageAllocator) -> Option<&mut PTE> {
        self.walk(va, false, allocator)
            .and_then(|pte| if pte.exists() { Some(pte) } else { None })
    }

    /// Unmaps the physical page at virtual address 'va'.
    /// If there is no physical page at that address, silently does nothing.
    ///
    /// Details:
    ///   - The ref count on the physical page should decrement.
    ///   - The physical page should be freed if the refcount reaches 0.
    ///   - The pg table entry corresponding to 'va' should be set to 0.
    ///     (if such a PTE exists)
    ///   - The TLB must be invalidated if you remove an entry from
    ///     the page table.
    fn remove(&mut self, va: VirtAddr, allocator: &mut PageAllocator) {
        match self.lookup(va, allocator) {
            None => (),
            Some(pte) => {
                PageDirectory::remove_pte(va, pte, allocator);
            }
        }
    }

    fn remove_pte(va: VirtAddr, pte: &mut PTE, allocator: &mut PageAllocator) {
        /// Invalidate a TLB entry, but only if the page tables being
        /// edited are the ones currently in use by the processor.
        fn tlb_invalidate(va: VirtAddr) {
            // Flush the entry only if we're modifying the current address space.
            // For now, there is only one address space, so always invalidate.
            x86::invlpg(va);
        }

        allocator.decref_pte(pte);
        pte.clear();
        tlb_invalidate(va);
    }

    /// Map the physical page 'pp' at virtual address 'va'.
    /// The permissions (the low 12 bits) of the page table entry
    /// should be set to 'perm|PTE_P'.
    ///
    /// Requirements
    ///   - If there is already a page mapped at 'va', it should be page_remove()d.
    ///   - If necessary, on demand, a page table should be allocated and inserted
    ///     into 'pgdir'.
    ///   - pp->pp_ref should be incremented if the insertion succeeds.
    ///   - The TLB must be invalidated if a page was formerly present at 'va'.
    ///
    /// RETURNS:
    ///   0 on success
    ///   -E_NO_MEM, if page table couldn't be allocated
    fn insert(&mut self, pa: PhysAddr, va: VirtAddr, perm: u32, allocator: &mut PageAllocator) {
        // TODO: should use Result
        let old_pte = self.walk(va, true, allocator).unwrap();
        // increment first to handle the corner case: the same PageInfo is re-inserted at the same virtual address
        let new_pte = PTE::new(pa, perm | PTE_P);
        allocator.incref_pte(&new_pte);
        if old_pte.exists() {
            PageDirectory::remove_pte(va, old_pte, allocator);
        }
        old_pte.set(new_pte.addr(), new_pte.attr());
    }
}

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

#[derive(Debug)]
#[repr(C)]
pub(crate) struct PDE(u32);

impl PDE {
    fn new(pa: PhysAddr, attr: u32) -> PDE {
        let mut pde = PDE(0);
        pde.set(pa, attr);
        pde
    }

    const fn empty() -> PDE {
        PDE(0)
    }

    fn exists(&self) -> bool {
        self.0 & PTE_P == 0x1
    }

    fn set(&mut self, pa: PhysAddr, attr: u32) {
        self.0 = pa.0 | attr;
    }

    fn table(&self) -> &mut PageTable {
        let va = PhysAddr(self.0 & 0xfffff000).to_va();
        unsafe { &mut *(va.0 as *mut PageTable) }
    }
}

#[repr(align(4096))]
#[repr(C)]
struct PageTable {
    entries: [PTE; NPTENTRIES],
}

impl Index<usize> for PageTable {
    type Output = PTE;
    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index]
    }
}

impl IndexMut<usize> for PageTable {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.entries[index]
    }
}

impl Index<PTX> for PageTable {
    type Output = PTE;
    fn index(&self, index: PTX) -> &Self::Output {
        let addr = (index.0).0 as usize;
        let addr = (addr >> 12) & 0x3ff;
        &self[addr]
    }
}

impl IndexMut<PTX> for PageTable {
    fn index_mut(&mut self, index: PTX) -> &mut Self::Output {
        let addr = (index.0).0 as usize;
        let addr = (addr >> 12) & 0x3ff;
        &mut self[addr]
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct PTX(VirtAddr);

#[derive(Debug)]
#[repr(C)]
struct PTE(u32);

impl PTE {
    fn new(pa: PhysAddr, attr: u32) -> PTE {
        let mut pte = PTE(0);
        pte.set(pa, attr);
        pte
    }

    fn exists(&self) -> bool {
        self.0 & PTE_P == 0x1
    }

    fn addr(&self) -> PhysAddr {
        PhysAddr(self.0 & 0xfffff000)
    }

    fn attr(&self) -> u32 {
        self.0 & 0x00000fff
    }

    fn set(&mut self, pa: PhysAddr, attr: u32) {
        self.0 = pa.0 | attr;
    }

    fn clear(&mut self) {
        self.0 = 0;
    }
}

fn round_up_u32(x: u32, base: u32) -> u32 {
    ((x - 1 + base) / base) * base
}

fn round_up_va(x: VirtAddr, base: u32) -> VirtAddr {
    VirtAddr(round_up_u32(x.0, base))
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
    let bss_end = VirtAddr(unsafe { &end as *const _ as u32 });
    let mut boot_allocator = BootAllocator::new(bss_end);
    let kern_pgdir_va = boot_allocator.alloc(PGSIZE);
    let kern_pgdir = unsafe { &mut *(kern_pgdir_va.0 as *mut PageDirectory) };
    println!("kern_pgdir: 0x{:x}", kern_pgdir_va.0);
    // memset(kern_pgdir, 0, PGSIZE);

    // Allocate an array of npages 'struct PageInfo's and store it in 'pages'.
    // The kernel uses this array to keep track of physical pages: for
    // each physical page, there is a corresponding struct PageInfo in this
    // array.  'npages' is the number of physical pages in memory.  Use memset
    // to initialize all fields of each struct PageInfo to 0.
    let page_info_size = mem::size_of::<PageInfo>();
    let pages = boot_allocator.alloc(npages * page_info_size as u32).0 as *mut PageInfo;

    // Allocate kernel heap
    println!("before: 0x{:x}", boot_allocator.alloc(0).0);
    let kheap = boot_allocator.alloc(KHEAP_SIZE as u32).0 as *mut PageInfo;
    println!("kheap: {:?}", kheap);
    println!("after: 0x{:x}", boot_allocator.alloc(0).0);

    // Now that we've allocated the initial kernel data structures, we set
    // up the list of free physical pages. Once we've done so, all further
    // memory management will go through the page_* functions. In
    // particular, we can now map memory using boot_map_region or page_insert
    let mut allocator = PageAllocator::new(pages, &mut boot_allocator, npages, npages_basemem);
    // memset(pages, 0, npages * sizeof (struct PageInfo));
    println!("pages: 0x{:?}", pages);

    println!("page_free_list: 0x{:?}", allocator.page_free_list);

    let x = kern_pgdir.walk(VirtAddr(0x00001000), false, &mut allocator);
    println!("pte: {:?}", x);
    let x = kern_pgdir.walk(VirtAddr(0x00001000), true, &mut allocator);
    println!("pte: {:?}", x);

    // Now we set up virtual memory

    // Map kernel heap
    // This mapping is not in neither xv6 nor jos.
    kern_pgdir.boot_map_region(
        VirtAddr(KHEAP_BASE),
        KHEAP_SIZE,
        VirtAddr(kheap as u32).to_pa(),
        PTE_P | PTE_W,
        &mut allocator,
    );

    // Use the physical memory that 'bootstack' refers to as the kernel
    // stack.  The kernel stack grows down from virtual address KSTACKTOP.
    // We consider the entire range from [KSTACKTOP-PTSIZE, KSTACKTOP)
    // to be the kernel stack, but break this into two pieces:
    //     * [KSTACKTOP-KSTKSIZE, KSTACKTOP) -- backed by physical memory
    //     * [KSTACKTOP-PTSIZE, KSTACKTOP-KSTKSIZE) -- not backed; so if
    //       the kernel overflows its stack, it will fault rather than
    //       overwrite memory.  Known as a "guard page".
    //     Permissions: kernel RW, user NONE
    kern_pgdir.boot_map_region(
        VirtAddr(KSTACKTOP - KSTKSIZE),
        KSTKSIZE as usize,
        PhysAddr(unsafe { &bootstack as *const _ as u32 }),
        PTE_P | PTE_W,
        &mut allocator,
    );

    // Map all of physical memory at KERNBASE.
    // Ie.  the VA range [KERNBASE, 2^32) should map to
    //      the PA range [0, 2^32 - KERNBASE)
    // We might not have 2^32 - KERNBASE bytes of physical memory, but
    // we just set up the mapping anyway.
    // Permissions: kernel RW, user NONE
    kern_pgdir.boot_map_region(
        VirtAddr(KERN_BASE),
        ((0xffffffff) - KERN_BASE + 1) as usize,
        PhysAddr(0),
        PTE_P | PTE_W,
        &mut allocator,
    );

    // Switch from the minimal entry page directory to the full kern_pgdir
    // page table we just created.	Our instruction pointer should be
    // somewhere between KERNBASE and KERNBASE+4MB right now, which is
    // mapped the same way by both page tables.
    x86::lcr3(VirtAddr(kern_pgdir as *const PageDirectory as u32).to_pa());

    // entry.S set the really important flags in cr0 (including enabling
    // paging).  Here we configure the rest of the flags that we care about.
    let mut cr0 = x86::rcr0();
    cr0 |= CR0_PE | CR0_PG | CR0_AM | CR0_WP | CR0_NE | CR0_MP;
    cr0 &= !(CR0_TS | CR0_EM);
    x86::lcr0(cr0);

    let x = kern_pgdir
        .lookup(VirtAddr(0xf0000000), &mut allocator)
        .unwrap();
    println!("pte: 0x{:x}", x.0);
    let x = kern_pgdir
        .lookup(VirtAddr(0xf0001000), &mut allocator)
        .unwrap();
    println!("pte: 0x{:x}", x.0);

    // insert and remove test
    let x = kern_pgdir.lookup(VirtAddr(0x00000000), &mut allocator);
    if x.is_some() {
        panic!("should be none");
    }
    let x = allocator.alloc(AllocFlag::AllocZero).unwrap();
    kern_pgdir.insert(x, VirtAddr(0x00000000), PTE_P | PTE_W, &mut allocator);
    let x = kern_pgdir.lookup(VirtAddr(0x00000000), &mut allocator);
    if x.is_none() {
        panic!("should be some");
    }
    kern_pgdir.remove(VirtAddr(0x00000000), &mut allocator);
    let x = kern_pgdir.lookup(VirtAddr(0x00000000), &mut allocator);
    if x.is_some() {
        panic!("should be none");
    }
}

// --------------------------------------------------------------
// Tracking of physical pages.
// The 'pages' array has one 'struct PageInfo' entry per physical page.
// Pages are reference counted, and free pages are kept on a linked list.
// --------------------------------------------------------------

#[derive(Debug)]
#[repr(C)]
struct PageInfo {
    pp_link: *mut PageInfo,
    pp_ref: u16,
}

// FIXME: how to represent it in rust way
struct PageAllocator {
    page_free_list: *mut PageInfo,
    pages: *mut PageInfo,
}

#[allow(dead_code)]
#[repr(u8)]
enum AllocFlag {
    None,
    AllocZero,
}

impl PageAllocator {
    fn new(
        pages: *mut PageInfo,
        ba: &mut BootAllocator,
        npages: u32,
        npages_basemem: u32,
    ) -> PageAllocator {
        let mut allocator = PageAllocator {
            page_free_list: null_mut(),
            pages: pages,
        };
        allocator.init(ba, npages, npages_basemem);
        allocator
    }

    /// Initialize page structure and memory free list.
    /// After this is done, NEVER use boot_alloc again.  ONLY use the page
    /// allocator functions below to allocate and deallocate physical
    /// memory via the page_free_list.
    fn init(&mut self, ba: &mut BootAllocator, npages: u32, npages_basemem: u32) {
        let first_free_page = ba.alloc(0).to_pa().0 / PGSIZE;
        for i in 0..npages {
            // skip the first 4 KB in case that we need real-mode IDT and BIOS structures.
            if i == 0 {
                continue;
            }
            // already used in kernel
            if i >= npages_basemem && i < first_free_page {
                continue;
            }
            let page = unsafe { &mut *(self.pages.add(i as usize)) };
            page.pp_ref = 0;
            page.pp_link = self.page_free_list;
            self.page_free_list = page as *mut PageInfo;
        }

        // FIXME later
        // It is necessary to reverse the order because
        // entry_pgdir doesn't map the higher addresses.
        unsafe {
            let mut prev = self.page_free_list;
            let mut cur = (*prev).pp_link;
            (*prev).pp_link = null_mut();
            while cur != null_mut() {
                let tmp = (*cur).pp_link;
                (*cur).pp_link = prev;
                prev = cur;
                cur = tmp;
            }
            self.page_free_list = prev;
        }
    }

    /// Allocates a physical page.  If (alloc_flags & ALLOC_ZERO), fills the entire
    /// returned physical page with '\0' bytes.  Does NOT increment the reference
    /// count of the page - the caller must do these if necessary (either explicitly
    /// or via page_insert).
    ///
    /// Be sure to set the pp_link field of the allocated page to NULL so
    /// page_free can check for double-free bugs.
    ///
    /// Returns NULL if out of free memory.
    fn alloc(&mut self, flag: AllocFlag) -> Option<PhysAddr> {
        unsafe {
            let pp = self.page_free_list;
            if pp == null_mut() {
                return None;
            }

            self.page_free_list = (*pp).pp_link;

            match flag {
                AllocFlag::AllocZero => {}
                _ => {}
            }
            // if (alloc_flags & ALLOC_ZERO) {
            //     memset(page2kva(pp), 0, PGSIZE);
            // }

            (*pp).pp_ref = 0;
            (*pp).pp_link = null_mut();

            Some(self.to_pa(pp))
        }
    }

    fn to_pa(&self, pp: *const PageInfo) -> PhysAddr {
        unsafe {
            let offset = pp.offset_from(self.pages) as u32;
            PhysAddr(offset << PGSHIFT)
        }
    }

    // TODO: summarize PDE and PTE
    fn incref_pte(&self, pte: &PTE) {
        let offset = (pte.0 >> PGSHIFT) as isize;
        let pp = unsafe { &mut *(self.pages.offset(offset)) };
        pp.pp_ref += 1;
    }

    fn incref_pde(&self, pde: &PDE) {
        let offset = (pde.0 >> PGSHIFT) as isize;
        let pp = unsafe { &mut *(self.pages.offset(offset)) };
        pp.pp_ref += 1;
    }

    fn decref_pte(&mut self, pte: &PTE) {
        let offset = (pte.0 >> PGSHIFT) as isize;
        let pp = unsafe { &mut *(self.pages.offset(offset)) };
        pp.pp_ref -= 1;
        if pp.pp_ref == 0 {
            self.free(pp);
        }
    }

    fn decref_pde(&mut self, pde: &PDE) {
        let offset = (pde.0 >> PGSHIFT) as isize;
        let pp = unsafe { &mut *(self.pages.offset(offset)) };
        pp.pp_ref -= 1;
        if pp.pp_ref == 0 {
            self.free(pp);
        }
    }

    /// Return a page to the free list.
    /// (This function should only be called when pp->pp_ref reaches 0.)
    fn free(&mut self, pp: *mut PageInfo) {
        unsafe {
            assert_ne!(pp, null_mut(), "pp should not be null");
            assert_eq!((*pp).pp_ref, 0, "pp_ref should be zero");
            assert_eq!((*pp).pp_link, null_mut(), "pp_link should be null");
            (*pp).pp_link = self.page_free_list;
            self.page_free_list = pp;
        }
    }
}
