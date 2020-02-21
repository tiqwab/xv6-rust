use crate::pmap::PhysAddr;
use core::mem;

/*
 * See MultiProcessor Specification (MP)
 * https://pdos.csail.mit.edu/6.828/2018/readings/ia32/MPspec.pdf
 */

/// MP Floating Pointer Structure
/// See MP 4.1
#[repr(C, packed)]
struct Mp {
    signature: [u8; 4],  // "_MP_"
    phys_addr: PhysAddr, // the physical address of the beginning of the MP configuration table.
    length: u8, // the length of the floating pointer structure table in paragraph (16-byte) units. This is always 0x01.
    spec_rev: u8, // the version number of the MP spec supported.
    checksum: u8, // all bytes must add up to 0.
    typ: u8,    // MP system config type
    imcrp: u8,  // set if IMCR is present and PIC Mode is implemented
    reserved: [u8; 3],
}

impl Mp {
    /// Search for the MP Floating Pointer Structure, which according to
    /// MP 4 is in one of the following three locations:
    /// 1) in the first KB of the EBDA;
    /// 2) if there is no EBDA, in the last KB of system base memory;
    /// 3) in the BIOS ROM between 0xF0000 and 0xFFFFF.
    unsafe fn new() -> Option<&'static Mp> {
        assert_eq!(mem::size_of::<Mp>(), 16);

        // BDA (BIOS Data Area)
        // ref. https://wiki.osdev.org/Memory_Map_(x86)
        let bda: *const u8 = PhysAddr(0x00000400).to_va().as_ptr();

        // The 16-bit segment of the EBDAs is USUALLY in the two bytes
        // starting at byte 0x0E 0f the BDA. 0 if not present.
        let seg = *(bda.offset(0x0e).cast::<u16>());
        if seg != 0 {
            // 1)
            let pa = PhysAddr((seg as u32) << 4); // translate from segment to phys addr
            if let Some(v) = Mp::search(pa, 1024) {
                return Some(v);
            }
        } else {
            // 2)
            // The size of base memory, in KB is in the two bytes
            // starting at 0x13 of the BDA.
            // (OSDev wiki might be wrong for it. This is "Base memory size in kbytes (0-640)" according to https://web.archive.org/web/20120130052813/http://www.nondot.org/sabre/os/files/Booting/BIOS_SEG.txt)
            let sz = *(bda.offset(0x13).cast::<u16>());
            let pa = PhysAddr((sz as u32) - 1024);
            if let Some(v) = Mp::search(pa, 1024) {
                return Some(v);
            }
        }

        // 3)
        Mp::search(PhysAddr(0xf0000), 0x10000)
    }

    /// Look for an MP structure in the len bytes at the physical address.
    unsafe fn search(base: PhysAddr, len: usize) -> Option<&'static Mp> {
        let mut mp = base.to_va().as_ptr::<Mp>();
        let end = base
            .to_va()
            .as_ptr::<u8>()
            .offset(len as isize)
            .cast::<Mp>();

        while mp < end {
            // "_MP_"
            if &(*mp).signature == &[0x5f, 0x4d, 0x50, 0x5f] {
                break;
            }
            mp = mp.add(1);
        }

        if mp != end {
            if check_sum(mp, mem::size_of::<Mp>()) {
                mp.as_ref()
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// MP Configuration Table Header
/// See MP 4.2
#[repr(C, packed)]
struct MpConf {
    signature: [u8; 4], // "PCMP"
    length: u16,        // the length of the base configuration table in bytes.
    version: u8,        // the version number of the MP specification.
    checksum: u8,
    product: [u8; 20],    // product id
    oem_table: PhysAddr,  // OEM table pointer
    oem_length: u16,      // OEM table length
    entry: u16,           // the number of entries in the variable portion of the base table
    lapic_addr: PhysAddr, // the physical address of local APIC
    xlength: u16,         // the length in bytes of the extended entries
    xchecksum: u8,        // the checksum for the extended entries
    reserved: u8,
    entries: [u8; 0], // table entries (the number of entries is in 'entry' field)
}

impl MpConf {
    unsafe fn new() -> Result<&'static MpConf, &'static str> {
        let mp = {
            let p = Mp::new().ok_or("MP floating pointer structure is not found")?;
            if p.phys_addr == PhysAddr(0) || p.typ != 0 {
                Err("SMP: Default configurations not implemented")
            } else {
                Ok(p)
            }
        }?;

        let conf = {
            let p = mp.phys_addr.to_va().as_ptr::<MpConf>().as_ref();
            let p = p.ok_or("null pointer")?;
            if &p.signature != &[0x50, 0x43, 0x4d, 0x50] {
                Err("SMP: Incorrect MP configuration table signature")
            } else {
                Ok(p)
            }
        }?;

        if !check_sum(conf, conf.length as usize) {
            return Err("SMP: Bad MP configuration checksum");
        }

        if conf.version != 1 && conf.version != 4 {
            return Err("SMP: Unsupported MP version");
        }

        let ptr_for_extended = (conf as *const MpConf).offset(conf.length as isize);
        if !check_sum(ptr_for_extended, conf.xlength as usize) {
            return Err("SMP: Bad MP configuration extended checksum");
        }

        Ok(conf)
    }
}

/*
 * Base MP Configuration Table Entries.
 * They are kinds of entries following MpConf.
 */

/// Processor Entries
#[repr(C, packed)]
struct MpProc {
    typ: u8,            // entry type (0 for Processor Entries)
    apicid: u8,         // local APIC id
    version: u8,        // local APIC version
    flags: u8,          // CPU flags
    signature: [u8; 4], // CPU signature
    feature: u32,       // feature flags from CPUID instruction
    reserved: [u8; 8],
}

unsafe fn check_sum<T>(mp: *const T, size: usize) -> bool {
    // checksum
    // Rust detects overflow, so accumulates as u32.
    let p = mp.cast::<u8>();
    let mut sum: u32 = 0;

    for i in 0..size {
        sum += p.offset(i as isize).read() as u32;
    }

    (sum & 0xff) == 0
}

pub(crate) fn mp_init() {
    let mp = unsafe { Mp::new().expect("mp should be found") };
    println!("mp found at {:p}", mp as *const Mp);
    let conf = unsafe { MpConf::new().unwrap() };
}
