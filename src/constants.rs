// FIXME: how to manage constant values (in rust as well as c and asm)

pub(crate) const KERN_BASE: u32 = 0xf0000000;
pub(crate) const PGSIZE: u32 = 4096;
pub(crate) const PGSHIFT: u32 = 12;
pub(crate) const PTE_U: u32 = 0x4;
pub(crate) const PTE_W: u32 = 0x2;
pub(crate) const PTE_P: u32 = 0x1;
pub(crate) const NPDENTRIES: usize = 1024;
pub(crate) const NPTENTRIES: usize = 1024;
pub(crate) const PTSIZE: usize = NPTENTRIES * (PGSIZE as usize);

pub(crate) const KSTACKTOP: u32 = KERN_BASE;
pub(crate) const KSTKSIZE: u32 = (8 * PGSIZE);
pub(crate) const KSTKGAP: u32 = (8 * PGSIZE);
pub(crate) const MMIOLIM: u32 = KSTACKTOP - (PTSIZE as u32);
pub(crate) const MMIOBASE: u32 = MMIOLIM - (PTSIZE as u32);
pub(crate) const ULIM: u32 = MMIOBASE;
// Assign kernel heap area instead of Cur. Page Table, RO PAGES, and RO ENVS in JOS
pub(crate) const KHEAP_BASE: u32 = ULIM - KHEAP_SIZE as u32;
pub(crate) const KHEAP_SIZE: usize = 3 * PTSIZE;

pub(crate) const UTOP: u32 = KHEAP_BASE;
pub(crate) const UXSTACKTOP: u32 = UTOP;
pub(crate) const USTACKTOP: u32 = UTOP - (2 * PGSIZE as u32);

// CR0
pub(crate) const CR0_PE: u32 = 0x0000001; // Protection Enable
pub(crate) const CR0_MP: u32 = 0x0000002; // Monitor coProcessor
pub(crate) const CR0_EM: u32 = 0x0000004; // Emulation
pub(crate) const CR0_TS: u32 = 0x0000008; // Task Switched
pub(crate) const CR0_ET: u32 = 0x0000010; // Extension Type
pub(crate) const CR0_NE: u32 = 0x0000020; // Numeric Error
pub(crate) const CR0_WP: u32 = 0x0010000; // Write Protect
pub(crate) const CR0_AM: u32 = 0x0040000; // Alignment Mask
pub(crate) const CR0_NW: u32 = 0x2000000; // Not Write through
pub(crate) const CR0_CD: u32 = 0x4000000; // Cache Disable
pub(crate) const CR0_PG: u32 = 0x8000000; // Paging

// EFLAGS register
pub(crate) const FL_CF: u32 = 1 << 0; // Carry Flag
pub(crate) const FL_PF: u32 = 1 << 2; // Parity Flag
pub(crate) const FL_AF: u32 = 1 << 4; // Auxiliary carry Flag
pub(crate) const FL_ZF: u32 = 1 << 6; // Zero Flag
pub(crate) const FL_SF: u32 = 1 << 7; // Sign Flag
pub(crate) const FL_TF: u32 = 1 << 8; // Trap Flag
pub(crate) const FL_IF: u32 = 1 << 9; // Interrupt Flag
pub(crate) const FL_DF: u32 = 1 << 10; // Direction Flag
pub(crate) const FL_OF: u32 = 1 << 11; // Overflow Flag
pub(crate) const FL_IOPL_MASK: u32 = (1 << 12) | (1 << 13); // I/O Privilege Level bitmask
pub(crate) const FL_IOPL_0: u32 = 0x0; // IOPL == 0
pub(crate) const FL_IOPL_1: u32 = 1 << 12; // IOPL == 1
pub(crate) const FL_IOPL_2: u32 = 1 << 13; // IOPL == 2
pub(crate) const FL_IOPL_3: u32 = (1 << 12) | (1 << 13); // IOPL == 3
pub(crate) const FL_NT: u32 = 1 << 14; // Nested Task
pub(crate) const FL_RF: u32 = 1 << 16; // Resume Flag
pub(crate) const FL_VM: u32 = 1 << 17; // Virtual 8086 mode
pub(crate) const FL_AC: u32 = 1 << 18; // Alignment Check
pub(crate) const FL_VIF: u32 = 1 << 19; // Virtual Interrupt Flag
pub(crate) const FL_VIP: u32 = 1 << 20; // Virtual Interrupt Pending
pub(crate) const FL_ID: u32 = 1 << 21; // ID flag
