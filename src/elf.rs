// ref. https://pdos.csail.mit.edu/6.828/2018/readings/elf.pdf

use crate::pmap::VirtAddr;
use core::mem;

pub(crate) const ELF_MAGIC: u32 = 0x464c457f;

pub(crate) struct ElfParser {
    binary: *const u8,
    elf: &'static Elf,
}

impl ElfParser {
    pub(crate) unsafe fn new(binary: *const u8) -> Option<ElfParser> {
        let elf_opt = Elf::new(binary);
        elf_opt.map(|elf| ElfParser { binary, elf })
    }

    pub(crate) unsafe fn program_headers(&self) -> ProghdrIter {
        let ptr = self.binary.offset(self.elf.e_phoff as isize);
        let hdr = Proghdr::new(ptr).expect("unknown ProghdrType");
        let remain = self.elf.e_phnum as usize;
        ProghdrIter { ptr, hdr, remain }
    }

    pub(crate) fn entry_point(&self) -> VirtAddr {
        self.elf.entry_point()
    }
}

/// ELF Header.
/// See Figure 1-3.
#[repr(C, packed)]
pub(crate) struct Elf {
    pub(crate) e_magic: u32, // should be 0x7f,'E','L','F' in little endian
    pub(crate) e_elf: [u8; 12],
    pub(crate) e_type: u16,
    pub(crate) e_machine: u16,
    pub(crate) e_version: u32,
    pub(crate) e_entry: u32, // the virtual address to which the system first transfers control
    pub(crate) e_phoff: u32, // the program header table's file offset in bytes
    pub(crate) e_shoff: u32, // the section header table's file offset in bytes
    pub(crate) e_flags: u32,
    pub(crate) e_ehsize: u16,    // ELF header's size in bytes
    pub(crate) e_phentsize: u16, // the size in bytes of one entry in the file's program header table
    pub(crate) e_phnum: u16,     // the number of entries in the program header table
    pub(crate) e_shentsize: u16, // the size in bytes of one entry in the file's section header table
    pub(crate) e_shnum: u16,     // the number of entries in the section header table
    pub(crate) e_shstrndx: u16,
}

impl Elf {
    pub(crate) unsafe fn new(binary: *const u8) -> Option<&'static Elf> {
        let elf = &(*(binary as *const Elf)) as &Elf;
        if elf.is_valid() {
            Some(elf)
        } else {
            None
        }
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.e_magic == ELF_MAGIC
    }

    pub(crate) fn entry_point(&self) -> VirtAddr {
        VirtAddr(self.e_entry)
    }
}

/// Program Header.
/// See Figure 2-1.
#[repr(C, packed)]
pub(crate) struct Proghdr {
    pub(crate) p_type: ProghdrType,
    pub(crate) p_offset: u32, // the offset from the beginning of the file at which the first byte of the segment resides
    pub(crate) p_vaddr: u32, // the virtual address at which the first byte of the segment resides in memory
    pub(crate) p_paddr: u32,
    pub(crate) p_filesz: u32, // the number of bytes in the file image of the segment
    pub(crate) p_memsz: u32,  // the number of bytes int the memory image of the segment
    pub(crate) p_flags: u32,
    pub(crate) p_align: u32,
}

impl Proghdr {
    unsafe fn new(ptr: *const u8) -> Option<&'static Proghdr> {
        let ptr = ptr as *const Proghdr;
        let raw_typ = *(ptr as *const u32);
        let typ_opt = ProghdrType::from_u32(raw_typ);
        match typ_opt {
            None => None,
            Some(_) => Some(&(*ptr)),
        }
    }
}

pub(crate) struct ProghdrIter<'a> {
    ptr: *const u8,
    hdr: &'a Proghdr,
    remain: usize,
}

impl<'a> Iterator for ProghdrIter<'a> {
    type Item = &'a Proghdr;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remain <= 0 {
            None
        } else {
            unsafe {
                let ph = Proghdr::new(self.ptr).expect("unknown ProghdrType");
                self.hdr = ph;
                self.remain -= 1;
                self.ptr = self.ptr.add(mem::size_of::<Proghdr>());
                Some(self.hdr)
            }
        }
    }
}

/// enum for p_type of Proghdr.
/// There are some types which don't exist in the spec but added by compiler.
/// ref. http://sugawarayusuke.hatenablog.com/entry/2017/04/09/213133
#[derive(Debug, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum ProghdrType {
    PtNull = 0,
    PtLoad = 1,
    PtDynamic = 2,
    PtInterp = 3,
    PtNote = 4,
    PtShlib = 5,
    PtPhdr = 6,
    PtGnuStack = 0x6474e551,
    PtLoproc = 0x70000000,
    PtHiproc = 0x7fffffff,
}

impl ProghdrType {
    fn from_u32(v: u32) -> Option<ProghdrType> {
        match v {
            _ if v == ProghdrType::PtNull as u32 => Some(ProghdrType::PtNull),
            _ if v == ProghdrType::PtLoad as u32 => Some(ProghdrType::PtLoad),
            _ if v == ProghdrType::PtDynamic as u32 => Some(ProghdrType::PtDynamic),
            _ if v == ProghdrType::PtInterp as u32 => Some(ProghdrType::PtInterp),
            _ if v == ProghdrType::PtNote as u32 => Some(ProghdrType::PtNote),
            _ if v == ProghdrType::PtShlib as u32 => Some(ProghdrType::PtShlib),
            _ if v == ProghdrType::PtPhdr as u32 => Some(ProghdrType::PtPhdr),
            _ if v == ProghdrType::PtGnuStack as u32 => Some(ProghdrType::PtGnuStack),
            _ if v == ProghdrType::PtLoproc as u32 => Some(ProghdrType::PtLoproc),
            _ if v == ProghdrType::PtHiproc as u32 => Some(ProghdrType::PtHiproc),
            _ => None,
        }
    }
}

// values for p_flags of Proghdr
pub(crate) const PROGHDR_FLAGS_X: u32 = 1 << 0; // Execute
pub(crate) const PROGHDR_FLAGS_W: u32 = 1 << 1; // Write
pub(crate) const PROGHDR_FLAGS_R: u32 = 1 << 2; // Read
pub(crate) const PROGHDR_FLAGS_MASKPROC: u32 = 1 << 31; // Unspecified

/// Section Header.
/// See Figure 1-8.
#[repr(C, packed)]
pub(crate) struct Secthdr {
    pub(crate) sh_name: u32,
    pub(crate) sh_type: SecthdrType,
    pub(crate) sh_flags: u32,
    pub(crate) sh_addr: u32,
    pub(crate) sh_offset: u32,
    pub(crate) sh_size: u32,
    pub(crate) sh_link: u32,
    pub(crate) sh_info: u32,
    pub(crate) sh_addralign: u32,
    pub(crate) sh_entsize: u32,
}

#[derive(Debug, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum SecthdrType {
    ShtNull = 0,
    ShtProgbits = 1,
    ShtSymtab = 2,
    ShtStrtab = 3,
    ShtRela = 4,
    ShtHash = 5,
    ShtDynamic = 6,
    ShtNote = 7,
    ShtNobits = 8,
    ShtRel = 9,
    ShtShlib = 10,
    ShtDynsym = 11,
    ShtLoproc = 0x70000000,
    ShtHiproc = 0x7fffffff,
    ShtLouser = 0x80000000,
    ShtHiuser = 0xffffffff,
}
