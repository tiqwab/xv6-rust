use crate::env::Env;
use crate::gdt::TaskState;
use crate::pmap::{PhysAddr, VirtAddr};
use crate::{lapic, x86};
use consts::*;
use core::mem;
use core::ptr::{null, null_mut, slice_from_raw_parts};

/*
 * See MultiProcessor Specification (MP)
 * https://pdos.csail.mit.edu/6.828/2018/readings/ia32/MPspec.pdf
 */

pub(crate) mod consts {
    // Table entry types
    pub(crate) const MP_PROC: u8 = 0x00; // One per processor
    pub(crate) const MP_BUS: u8 = 0x01; // One per bus
    pub(crate) const MP_IOAPIC: u8 = 0x02; // One per I/O APIC
    pub(crate) const MP_IOINTR: u8 = 0x03; // One per bus interrupt source
    pub(crate) const MP_LINTR: u8 = 0x04; // One per system interrupt source

    // Bit flags of MpProc.flag
    pub(crate) const MPPROC_FLAGS_BP: u8 = (1 << 1);

    // Maximum Number of CPUs
    pub(crate) const MAX_NUM_CPU: usize = 8;
}

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

impl MpProc {
    fn is_bsp(&self) -> bool {
        self.flags & MPPROC_FLAGS_BP != 0
    }
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

/// Per-CPU state
#[repr(C)]
pub(crate) struct CpuInfo {
    pub(crate) cpu_id: u8,
    cpu_status: CpuStatus,
    cpu_env: *mut Env,
    cpu_ts: TaskState,
}

impl CpuInfo {
    const fn empty() -> CpuInfo {
        CpuInfo {
            cpu_id: 0,
            cpu_status: CpuStatus::CpuUnused,
            cpu_env: null_mut(),
            cpu_ts: TaskState::empty(),
        }
    }

    pub(crate) fn is_started(&self) -> bool {
        self.cpu_status == CpuStatus::CpuStarted
    }

    pub(crate) fn init_ts(&mut self, esp0: VirtAddr, ss0: u16, iomb: u16) -> &TaskState {
        self.cpu_ts.init(esp0, ss0, iomb);
        &self.cpu_ts
    }

    pub(crate) fn started(&mut self) {
        let p = ((&mut self.cpu_status) as *mut CpuStatus).cast::<u32>();
        let v = CpuStatus::CpuStarted as u32;
        x86::xchg(p, v);
    }

    pub(crate) fn cur_env(&self) -> Option<&Env> {
        unsafe { self.cpu_env.as_ref() }
    }

    pub(crate) fn cur_env_mut(&mut self) -> Option<&mut Env> {
        unsafe { self.cpu_env.as_mut() }
    }

    pub(crate) fn set_env(&mut self, env: *mut Env) {
        self.cpu_env = env;
    }
}

// Why it requires 4 bytes?
#[derive(PartialEq, Eq)]
#[repr(u32)]
enum CpuStatus {
    CpuUnused = 0,
    CpuStarted,
    CpuHalted,
}

/// CPU states
static mut CPUS: [CpuInfo; MAX_NUM_CPU] = [CpuInfo::empty(); MAX_NUM_CPU];
/// Total number of CPUS in the system
static mut NCPU: usize = 0;
/// Poniter to bsp (bootstrap processor)
static mut BOOT_CPU: *mut CpuInfo = null_mut();
/// Physical MMIO address of the local APIC
static mut LAPIC_ADDR: Option<PhysAddr> = None;

/// ref. MP Appendix B. Operating System Programming Guidelines (after B.4)
pub(crate) unsafe fn mp_init() {
    let mp = Mp::new().expect("mp should be found");
    println!("mp found at {:p}", mp as *const Mp);

    let conf = MpConf::new().unwrap();
    LAPIC_ADDR = Some(conf.lapic_addr);
    let mut ismp = true;

    let mut p = conf.entries.as_ptr();
    for _ in 0..conf.entry {
        let typ = *p;
        if typ == MP_PROC {
            let proc = &(*(p.cast::<MpProc>()));
            if proc.is_bsp() {
                BOOT_CPU = &mut CPUS[NCPU];
            }
            if NCPU < MAX_NUM_CPU {
                CPUS[NCPU].cpu_id = NCPU as u8;
                NCPU += 1;
            } else {
                println!("SMP: too many CPUs, CPU {} disabled", proc.apicid);
            }
            p = p.offset(mem::size_of::<MpProc>() as isize);
        } else if typ == MP_BUS {
            p = p.offset(8);
        } else if typ == MP_IOAPIC {
            p = p.offset(8);
        } else if typ == MP_IOINTR {
            p = p.offset(8);
        } else if typ == MP_LINTR {
            p = p.offset(8);
        } else {
            println!("mpinit: unknown config type: {:x}", typ);
            ismp = false;
            break;
        }
    }

    (&mut (*BOOT_CPU)).cpu_status = CpuStatus::CpuStarted;

    if !ismp {
        // Didn't like what we found; fall back to no MP.
        NCPU = 1;
        LAPIC_ADDR = None;
        println!("SMP: configuration not found, SMP disabled");
        return;
    }
    println!("SMP: CPU {} found {} CPU(s)", (&(*BOOT_CPU)).cpu_id, NCPU);

    if mp.imcrp > 0 {
        println!("SMP: Setting IMCR to switch from PIC mode to symmetric I/O mode");
        imcr_pic_to_apic();
    }

    println!("SMP: lapic_addr: 0x{:x}", LAPIC_ADDR.unwrap().0);
}

/// Handle interrupt mode configuration register (IMCR).
/// Switch to getting interrupts from the LAPIC if the hardware implements PIC mode.
/// ref. https://github.com/torvalds/linux/blob/54dedb5b571d2fb0d65c3957ecfa9b32ce28d7f0/arch/x86/kernel/apic/apic.c#L119
///
/// This is to change mode from PIC Mode to Virtual Wire Mode (or Symmetric I/O Mode eventually)?
/// ref. MP 3.6.2.1 PIC Mode
#[inline]
fn imcr_pic_to_apic() {
    // Select IMCR register
    x86::outb(0x22, 0x70);
    // NMI and 8259 INTR go through APIC
    let orig = x86::inb(0x23);
    x86::outb(0x23, orig | 0x01);
}

pub(crate) fn lapic_addr() -> Option<PhysAddr> {
    unsafe { LAPIC_ADDR.clone() }
}

pub(crate) fn this_cpu() -> &'static CpuInfo {
    unsafe { &CPUS[lapic::cpu_num() as usize] }
}

pub(crate) fn this_cpu_mut() -> &'static mut CpuInfo {
    unsafe { &mut CPUS[lapic::cpu_num() as usize] }
}

pub(crate) fn boot_cpu() -> &'static CpuInfo {
    unsafe { BOOT_CPU.as_ref().expect("BOOT_CPU should be exist") }
}

pub(crate) fn cpus() -> &'static [CpuInfo] {
    unsafe {
        let p = CPUS.as_ptr();
        let ncpus = NCPU;
        &(*slice_from_raw_parts(p, ncpus))
    }
}
