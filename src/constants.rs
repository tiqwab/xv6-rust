// FIXME: how to manage constant values (in rust as well as c and asm)

pub(crate) const KERN_BASE: u32 = 0xf0000000;
pub(crate) const PGSIZE: u32 = 4096;
pub(crate) const PTE_U: u32 = 0x4;
pub(crate) const PTE_P: u32 = 0x1;
pub(crate) const NPDENTRIES: usize = 1024;
pub(crate) const NPTENTRIES: usize = 1024;
pub(crate) const PTSIZE: usize = NPTENTRIES * (PGSIZE as usize);

pub(crate) const KSTACKTOP: u32 = KERN_BASE;
pub(crate) const MMIOLIM: u32 = KSTACKTOP - (PTSIZE as u32);
pub(crate) const MMIOBASE: u32 = MMIOLIM - (PTSIZE as u32);
pub(crate) const ULIM: u32 = MMIOBASE;
pub(crate) const UVPT: u32 = ULIM - (PTSIZE as u32);
