use core::mem;
use core::ops::{Add, AddAssign, Deref, DerefMut, Index, IndexMut, Sub};
use core::ptr::{null, null_mut, slice_from_raw_parts};

use crate::constants::*;
use crate::kclock;
use crate::mpconfig::consts::MAX_NUM_CPU;
use crate::spinlock::Mutex;
use crate::x86;
use alloc::boxed::Box;

extern "C" {
    static end: u32;
    static bootstack: u32;
}

// This MUST be initialized first with `init()`
struct KernelPageDirectory(*mut PageDirectory);
// Get the lock of KERN_PGDIR first if you use both of KERN_PGDIR and PAGE_ALLOCATOR.
static KERN_PGDIR: Mutex<KernelPageDirectory> = Mutex::new(KernelPageDirectory(null_mut()));

unsafe impl Send for KernelPageDirectory {}
unsafe impl Sync for KernelPageDirectory {}

impl KernelPageDirectory {
    fn init(&mut self, pgdir: *mut PageDirectory) {
        self.0 = pgdir;
    }

    fn paddr(&self) -> PhysAddr {
        VirtAddr(self.0 as u32).to_pa()
    }
}

impl Deref for KernelPageDirectory {
    type Target = PageDirectory;

    fn deref(&self) -> &PageDirectory {
        unsafe { &*self.0 }
    }
}

impl DerefMut for KernelPageDirectory {
    fn deref_mut(&mut self) -> &mut PageDirectory {
        unsafe { &mut *self.0 }
    }
}

// MUST be initialized first with `init()`
// Get the lock of KERN_PGDIR first if you use both of KERN_PGDIR and PAGE_ALLOCATOR.
static PAGE_ALLOCATOR: Mutex<PageAllocator> = Mutex::new(PageAllocator {
    page_free_list: null_mut(),
    pages: null_mut(),
});

#[repr(align(4096))]
pub(crate) struct CpuStack([u8; KSTKSIZE as usize]);
// type CpuStack = [u8; KSTKSIZE as usize];
type CpuStacks = [CpuStack; MAX_NUM_CPU];
static mut PERCPU_KSTACKS: CpuStacks = [CpuStack([0; KSTKSIZE as usize]); MAX_NUM_CPU];

impl CpuStack {
    pub(crate) fn as_ptr(&self) -> *const CpuStack {
        self as *const CpuStack
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut CpuStack {
        self as *mut CpuStack
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct VirtAddr(pub(crate) u32);

impl VirtAddr {
    /// VirtualAddr in kernel can be converted into PhysAddr.
    pub(crate) fn to_pa(&self) -> PhysAddr {
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

    pub(crate) fn round_up(&self, size: usize) -> VirtAddr {
        VirtAddr(round_up_u32(self.0, size as u32))
    }

    pub(crate) fn round_down(&self, size: usize) -> VirtAddr {
        VirtAddr(round_down_u32(self.0, size as u32))
    }

    pub(crate) fn as_ptr<T>(&self) -> *const T {
        self.0 as *const T
    }

    pub(crate) fn as_mut_ptr<T>(&self) -> *mut T {
        self.0 as *mut T
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

impl AddAssign<u32> for VirtAddr {
    fn add_assign(&mut self, rhs: u32) {
        self.0 += rhs;
    }
}

impl AddAssign<usize> for VirtAddr {
    fn add_assign(&mut self, rhs: usize) {
        self.0 += rhs as u32;
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

impl Sub for VirtAddr {
    type Output = usize;

    fn sub(self, rhs: Self) -> Self::Output {
        assert!(self.0 > rhs.0, "cannot subtract since rhs is larger");
        (self.0 - rhs.0) as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PhysAddr(pub(crate) u32);

impl PhysAddr {
    pub(crate) fn to_va(&self) -> VirtAddr {
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
                let next = self.bss_end.round_up(PGSIZE as usize);
                self.next_free = Some((next + n).round_up(PGSIZE as usize));
                next
            }
            Some(next) => {
                self.next_free = Some((next + n).round_up(PGSIZE as usize));
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(C)]
pub(crate) struct PDX(VirtAddr);

impl PDX {
    pub(crate) fn new(va: VirtAddr) -> PDX {
        let aligned_va = va.round_down(PGSIZE as usize);
        PDX(aligned_va)
    }
}

impl Add<u32> for PDX {
    type Output = ();

    fn add(self, rhs: u32) -> Self::Output {
        PDX(self.0 + rhs * PGSIZE);
    }
}

impl AddAssign<u32> for PDX {
    fn add_assign(&mut self, rhs: u32) {
        self.0 += rhs * PGSIZE;
    }
}

impl PageDirectory {
    pub(crate) const fn new() -> PageDirectory {
        PageDirectory {
            entries: [PDE::empty(); NPDENTRIES],
        }
    }

    pub(crate) fn new_for_user() -> Box<PageDirectory> {
        let mut pgdir = PageDirectory::new();
        let kern_pgdir = KERN_PGDIR.lock();
        // Copy kernel mapping
        for (i, kern_pde) in kern_pgdir.entries.iter().enumerate() {
            if kern_pde.exists() {
                let pde = PDE(kern_pde.0);
                pgdir.entries[i] = pde;
            }
        }
        Box::new(pgdir)
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
        let pdx = PDX::new(va);
        let pde = &mut self[pdx];
        if !pde.exists() {
            if !should_create {
                return None;
            }
            let pa = allocator.alloc(AllocFlag::AllocZero).expect("alloc failed");
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

    /// Allocate len bytes of physical memory for environment env,
    /// and map it at virtual address va in the environment's address space.
    /// Does not zero or otherwise initialize the mapped pages in any way.
    /// Pages should be writable by user and kernel.
    /// Panic if any allocation attempt fails.
    pub(crate) fn region_alloc(&mut self, va: VirtAddr, len: usize) {
        let mut allocator = PAGE_ALLOCATOR.lock();
        let start_va = va.round_down(PGSIZE as usize);
        let end_va = va.add(len).round_up(PGSIZE as usize);

        let mut va = start_va;
        while va < end_va {
            let pa = allocator.alloc(AllocFlag::None).unwrap();
            self.insert(pa, va, PTE_U | PTE_W, &mut *allocator);
            va += PGSIZE;
        }
    }

    pub(crate) fn vaddr(&self) -> VirtAddr {
        VirtAddr(self as *const PageDirectory as u32)
    }

    pub(crate) fn paddr(&mut self) -> Option<PhysAddr> {
        self.convert_to_pa(self.vaddr())
    }

    /// Convert a virtual address to a physical address.
    /// Return None if there is not page mapping.
    pub(crate) fn convert_to_pa(&mut self, va: VirtAddr) -> Option<PhysAddr> {
        let mut allocator = PAGE_ALLOCATOR.lock();
        self.lookup(va, &mut *allocator)
            .map(|pte| pte.addr() + (va.0 & 0xfff))
    }

    /// Unmaps PDE as well as all PTEs of the page table specified by the PDE.
    pub(crate) fn remove_pde(&mut self, pdx: PDX) {
        let pde = &self[pdx];
        let mut allocator = PAGE_ALLOCATOR.lock();

        let pt = pde.table();
        for i in 0..NPTENTRIES {
            let pte = &mut pt[i];
            if pte.exists() {
                let va = VirtAddr((pdx.0).0 | ((i as u32) * PGSIZE));
                PageDirectory::remove_pte(va, pte, &mut *allocator);
            }
        }

        let pde = &mut self[pdx];
        allocator.decref_pde(pde);
        pde.clear();
    }

    /// Check that an environment is allowed to access the range of memory
    /// [va, va+len) with permissions 'perm | PTE_P'.
    /// Normally 'perm' will contain PTE_U at least, but this is not required.
    /// 'va' and 'len' need not be page-aligned; you must test every page that
    /// contains any of that range.  You will test either 'len/PGSIZE',
    /// 'len/PGSIZE + 1', or 'len/PGSIZE + 2' pages.
    ///
    /// A user program can access a virtual address if (1) the address is below
    /// ULIM, and (2) the page table gives it permission.  These are exactly
    /// the tests you should implement here.
    ///
    /// If there is an error, Return Err(addr), which is the first erroneous virtual address.
    /// Returns Ok if the user program can access this range of addresses.
    pub(crate) fn user_mem_check(
        &mut self,
        orig_va: VirtAddr,
        orig_len: usize,
        perm: u32,
    ) -> Result<(), VirtAddr> {
        let mut allocator = PAGE_ALLOCATOR.lock();
        let start_va = orig_va.round_down(PGSIZE as usize);
        let end_va = (orig_va + orig_len).round_up(PGSIZE as usize);

        let mut va = start_va;
        while va < end_va {
            if va > VirtAddr(ULIM) {
                return Err(va);
            }
            let pte_opt = self.walk(va, false, &mut *allocator);
            match pte_opt {
                None => return Err(va),
                Some(pte) if pte.attr() & perm != perm => return Err(va),
                _ => (),
            }
            va += PGSIZE;
        }

        return Ok(());
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

    pub(crate) fn exists(&self) -> bool {
        self.0 & PTE_P == 0x1
    }

    fn set(&mut self, pa: PhysAddr, attr: u32) {
        self.0 = pa.0 | attr;
    }

    fn table(&self) -> &mut PageTable {
        let va = PhysAddr(self.0 & 0xfffff000).to_va();
        unsafe { &mut *(va.0 as *mut PageTable) }
    }

    fn clear(&mut self) {
        self.0 = 0;
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

fn round_down_u32(x: u32, base: u32) -> u32 {
    (x / base) * base
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

/// Reserve size bytes in the MMIO region and map [pa,pa+size) at this
/// location. Return the base of the reserved region. size does *not*
/// have to be multiple of PGSIZE.
pub(crate) fn mmio_map_region(start_pa: PhysAddr, orig_size: usize) -> VirtAddr {
    // Where to start the next region. Initially, this is the
    // beginning of the MMIO region. Because this is static, its
    // value will be preserved between calls to mmio_map_region
    // (just like nextfree in boot_alloc).
    static mut START_VA: VirtAddr = VirtAddr(MMIOBASE);

    // Reserve size bytes of virtual memory starting at base and
    // map physical pages [pa,pa+size) to virtual addresses
    // [base,base+size). Since this is device memory and not
    // regular DRAM, you'll have to tell the CPU that it isn't
    // safe to cache access to this memory.  Luckily, the page
    // tables provide bits for this purpose; simply create the
    // mapping with PTE_PCD|PTE_PWT (cache-disable and
    // write-through) in addition to PTE_W. (If you're interested
    // in more details on this, see section 10.5 of IA32 volume
    // 3A.)
    //
    // Be sure to round size up to a multiple of PGSIZE and to
    // handle if this reservation would overflow MMIOLIM (it's
    // okay to simply panic if this happens).
    unsafe {
        let mut pgdir = KERN_PGDIR.lock();
        let mut allocator = PAGE_ALLOCATOR.lock();

        let start_va = START_VA;
        let end_va = (start_va + orig_size).round_up(PGSIZE as usize);
        if end_va > VirtAddr(MMIOLIM) {
            panic!("too lage mmio_map_region");
        }
        let size = end_va - start_va;
        let perm = PTE_W | PTE_PCD | PTE_PWT;

        pgdir.boot_map_region(start_va, size, start_pa, perm, &mut *allocator);
        START_VA = end_va;

        start_va
    }
}

pub fn mem_init() {
    // Find out how much memory the machine has (npages & npages_basemem).
    let (npages, npages_basemem) = i386_detect_memory();

    // create initial page directory.
    let bss_end = VirtAddr(unsafe { &end as *const _ as u32 });
    let mut boot_allocator = BootAllocator::new(bss_end);
    let kern_pgdir_va = boot_allocator.alloc(PGSIZE);
    let mut kern_pgdir = KERN_PGDIR.lock();
    kern_pgdir.init(kern_pgdir_va.0 as *mut PageDirectory);
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
    let mut allocator = PAGE_ALLOCATOR.lock();
    allocator.init(pages, &mut boot_allocator, npages, npages_basemem);
    println!("pages: 0x{:?}", pages);

    println!("page_free_list: 0x{:?}", allocator.page_free_list);

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

    // Initialize the SMP-related parts of the memory map.
    mem_init_mp(&mut *kern_pgdir, &mut allocator);

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
    x86::lcr3(kern_pgdir.paddr());

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

/// Modify mappings in kern_pgdir to support SMP
///   - Map the per-CPU stacks in the region [KSTACKTOP-PTSIZE, KSTACKTOP)
fn mem_init_mp(kern_pgdir: &mut PageDirectory, allocator: &mut PageAllocator) {
    // Map per-CPU stacks starting at KSTACKTOP, for up to 'NCPU' CPUs.
    //
    // For CPU i, use the physical memory that 'percpu_kstacks[i]' refers
    // to as its kernel stack. CPU i's kernel stack grows down from virtual
    // address kstacktop_i = KSTACKTOP - i * (KSTKSIZE + KSTKGAP), and is
    // divided into two pieces, just like the single stack you set up in
    // mem_init:
    //     * [kstacktop_i - KSTKSIZE, kstacktop_i)
    //          -- backed by physical memory
    //     * [kstacktop_i - (KSTKSIZE + KSTKGAP), kstacktop_i - KSTKSIZE)
    //          -- not backed; so if the kernel overflows its stack,
    //             it will fault rather than overwrite another CPU's stack.
    //             Known as a "guard page".
    //     Permissions: kernel RW, user NONE

    for i in 0..MAX_NUM_CPU {
        let start_va = VirtAddr(KSTACKTOP - (KSTKSIZE + KSTKGAP) * (i as u32) - KSTKSIZE);
        let start_pa = unsafe { VirtAddr(&PERCPU_KSTACKS[i] as *const _ as u32).to_pa() };
        kern_pgdir.boot_map_region(
            start_va,
            KSTKSIZE as usize,
            start_pa,
            PTE_P | PTE_W,
            allocator,
        );
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
// This MUST be protected by Mutex
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

unsafe impl Send for PageAllocator {}
unsafe impl Sync for PageAllocator {}

impl PageAllocator {
    /// Initialize page structure and memory free list.
    /// After this is done, NEVER use boot_alloc again.  ONLY use the page
    /// allocator functions below to allocate and deallocate physical
    /// memory via the page_free_list.
    fn init(
        &mut self,
        pages: *mut PageInfo,
        ba: &mut BootAllocator,
        npages: u32,
        npages_basemem: u32,
    ) {
        self.page_free_list = null_mut();
        self.pages = pages;

        let first_free_page = ba.alloc(0).to_pa().0 / PGSIZE;
        for i in 0..npages {
            // skip the first 4 KB in case that we need real-mode IDT and BIOS structures.
            if i == 0 {
                continue;
            }

            // i == 7, 8 (around 0x7c00 as physical address) is used by boot loader,
            // but it is no longer required
            // if i == 7 || i == 8 {
            //     continue;
            // }

            // already used in kernel
            if i >= npages_basemem && i < first_free_page {
                continue;
            }

            // assume that the length of codes at mp_entry is less than PGSIZE
            if (i * PGSIZE) < (MPENTRY_PADDR + PGSIZE) && ((i + 1) * PGSIZE) >= MPENTRY_PADDR {
                continue;
            }

            let page = unsafe { &mut *(self.pages.add(i as usize)) };
            // println!("page[{}]: {:?}", i, page);
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

pub(crate) fn percpu_kstacks() -> &'static [CpuStack] {
    unsafe { &*slice_from_raw_parts(PERCPU_KSTACKS.as_ptr(), MAX_NUM_CPU) }
}

#[inline]
pub(crate) fn load_kern_pgdir() {
    let kern_pgdir = KERN_PGDIR.lock();
    x86::lcr3(kern_pgdir.paddr());
}
