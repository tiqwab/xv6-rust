use crate::pmap::PhysAddr;
use core::mem;

/// See MultiProcessor Specification (MP)
/// https://pdos.csail.mit.edu/6.828/2018/readings/ia32/MPspec.pdf

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
            // checksum
            // Rust detects overflow, so accumulates as u32.
            let p = mp.cast::<u8>();
            let size = mem::size_of::<Mp>();
            let mut sum: u32 = 0;

            for i in 0..size {
                sum += p.offset(i as isize).read() as u32;
            }

            if (sum & 0xff) != 0 {
                None
            } else {
                mp.as_ref()
            }
        } else {
            None
        }
    }
}

pub(crate) fn mp_init() {
    let mp = unsafe { Mp::new().expect("mp should be found") };
    println!("mp found at {:p}", mp as *const Mp);
}
