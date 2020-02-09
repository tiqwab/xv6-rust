// ref. https://pdos.csail.mit.edu/6.828/2018/readings/elf.pdf

#[allow(camel_case)]

/// ELF Header.
/// See Figure 1-3.
#[repr(C, packed)]
struct Elf {
    e_magic: u32, // should be 0x7f,'E','L','F' in little endian
    e_elf: [u8; 12],
    e_type: u32,
    e_machine: u32,
    e_version: u32,
    e_entry: u32, // the virtual address to which the system first transfers control
    e_phoff: u32, // the program header table's file offset in bytes
    e_shoff: u32, // the section header table's file offset in bytes
    e_flags: u32,
    e_ehsize: u16,    // ELF header's size in bytes
    e_phentsize: u16, // the size in bytes of one entry in the file's program header table
    e_phnum: u16,     // the number of entries in the program header table
    e_shentsize: u16, // the size in bytes of one entry in the file's section header table
    e_shnum: u16,     // the number of entries in the section header table
    e_shstrndx: u16,
}

/// Program Header.
/// See Figure 2-1.
#[repr(C, packed)]
struct Proghdr {
    p_type: ProghdrType,
    p_offset: u32, // the offset from the beginning of the file at which the first byte of the segment resides
    p_vaddr: u32,  // the virtual address at which the first byte of the segment resides in memory
    p_paddr: u32,
    p_filesz: u32, // the number of bytes in the file image of the segment
    p_memsz: u32,  // the number of bytes int the memory image of the segment
    p_flags: u32,
    p_align: u32,
}

#[repr(u32)]
enum ProghdrType {
    PtNull = 0,
    PtLoad = 1,
    PtDynamic = 2,
    PtInterp = 3,
    PtNote = 4,
    PtShlib = 5,
    PtPhdr = 6,
    PtLoproc = 0x70000000,
    PtHiproc = 0x7fffffff,
}

// values for p_flags of Proghdr
const PROGHDR_FLAGS_X: u32 = 1 << 0; // Execute
const PROGHDR_FLAGS_W: u32 = 1 << 1; // Write
const PROGHDR_FLAGS_R: u32 = 1 << 2; // Read
const PROGHDR_FLAGS_MASKPROC: u32 = 1 << 31; // Unspecified

/// Section Header.
/// See Figure 1-8.
#[repr(C, packed)]
struct Secthdr {
    sh_name: u32,
    sh_type: SecthdrType,
    sh_flags: u32,
    sh_addr: u32,
    sh_offset: u32,
    sh_size: u32,
    sh_link: u32,
    sh_info: u32,
    sh_addralign: u32,
    sh_entsize: u32,
}

#[repr(u32)]
enum SecthdrType {
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
