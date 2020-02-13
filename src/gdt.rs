// The part of this file comes from jos (kern/env.c) and redox (kernel/src/arch/x86_64/gdt.rs).
// See COPYRIGHT for copyright information.

use core::ptr::null;

use crate::pmap::{PhysAddr, VirtAddr};
use crate::x86;
use consts::*;

pub(crate) mod consts {
    pub const GDT_NULL: usize = 0x0;
    pub const GDT_KERNEL_CODE: usize = 0x8;
    pub const GDT_KERNEL_DATA: usize = 0x10;
    pub const GDT_USER_CODE: usize = 0x18;
    pub const GDT_USER_DATA: usize = 0x20;

    pub const GDT_A_PRESENT: u8 = 1 << 7;
    pub const GDT_A_RING_0: u8 = 0 << 5;
    pub const GDT_A_RING_1: u8 = 1 << 5;
    pub const GDT_A_RING_2: u8 = 2 << 5;
    pub const GDT_A_RING_3: u8 = 3 << 5;
    pub const GDT_A_SYSTEM: u8 = 1 << 4; // 0 for system, 1 for code or data
    pub const GDT_A_EXECUTABLE: u8 = 1 << 3; // set for code segment (executable segment)
    pub const GDT_A_CONFORMING: u8 = 1 << 2; // conforming for code segment
    pub const GDT_A_PRIVILEGE: u8 = 1 << 1; // readable for code segment, writable for data segment
    pub const GDT_A_DIRTY: u8 = 1;

    // pub const GDT_A_TSS_AVAIL: u8 = 0x9;
    // pub const GDT_A_TSS_BUSY: u8 = 0xB;

    pub const GDT_F_PAGE_SIZE: u8 = 1 << 7;
    pub const GDT_F_PROTECTED_MODE: u8 = 1 << 6;
    // pub const GDT_F_LONG_MODE: u8 = 1 << 5;
}

type GlobalDescriptorTable = [SegDesc; 6];

/// Global descriptor table.
///
/// Set up global descriptor table (GDT) with separate segments for
/// kernel mode and user mode.  Segments serve many purposes on the x86.
/// We don't use any of their memory-mapping capabilities, but we need
/// them to switch privilege levels.
///
/// The kernel and user segments are identical except for the DPL.
/// To load the SS register, the CPL must equal the DPL.  Thus,
/// we must duplicate the segments for the user and the kernel.
///
/// In particular, the last argument to the SEG macro used in the
/// definition of gdt specifies the Descriptor Privilege Level (DPL)
/// of that descriptor: 0 for kernel and 3 for user.
pub(crate) static GDT: GlobalDescriptorTable = [
    // NULL
    SegDesc::new(0x0, 0x0, 0x0, 0x0),
    // kernel code segment
    SegDesc::new(
        0x0,
        0xffffffff,
        GDT_A_PRESENT | GDT_A_RING_0 | GDT_A_SYSTEM | GDT_A_EXECUTABLE | GDT_A_PRIVILEGE,
        GDT_F_PAGE_SIZE | GDT_F_PROTECTED_MODE,
    ),
    // kernel data segment
    SegDesc::new(
        0x0,
        0xffffffff,
        GDT_A_PRESENT | GDT_A_RING_0 | GDT_A_SYSTEM | GDT_A_PRIVILEGE,
        GDT_F_PAGE_SIZE | GDT_F_PROTECTED_MODE,
    ),
    // user code segment
    SegDesc::new(
        0x0,
        0xffffffff,
        GDT_A_PRESENT | GDT_A_RING_3 | GDT_A_SYSTEM | GDT_A_EXECUTABLE | GDT_A_PRIVILEGE,
        GDT_F_PAGE_SIZE | GDT_F_PROTECTED_MODE,
    ),
    // user data segment
    SegDesc::new(
        0x0,
        0xffffffff,
        GDT_A_PRESENT | GDT_A_RING_3 | GDT_A_SYSTEM | GDT_A_PRIVILEGE,
        GDT_F_PAGE_SIZE | GDT_F_PROTECTED_MODE,
    ),
    // tss, initialized in trap_init_percpu()
    SegDesc::new(0x0, 0x0, 0x0, 0x0),
];

#[repr(C, packed)]
pub(crate) struct SegDesc {
    pub(crate) limitl: u16,
    pub(crate) offsetl: u16,
    pub(crate) offsetm: u8,
    pub(crate) access: u8,
    pub(crate) flags_limith: u8,
    pub(crate) offseth: u8,
}

impl SegDesc {
    const fn new(offset: u32, limit: u32, access: u8, flags: u8) -> SegDesc {
        SegDesc {
            limitl: (limit & 0xffff) as u16,
            offsetl: (offset & 0xffff) as u16,
            offsetm: ((offset >> 16) & 0xff) as u8,
            access,
            flags_limith: (flags & 0xf0) | (((limit >> 16) & 0xff) as u8 & 0x0f),
            offseth: ((offset >> 24) & 0xff) as u8,
        }
    }
}

#[repr(C, packed)]
pub(crate) struct TaskState {
    ts_link: u32,      // Okd ts selector
    ts_esp0: VirtAddr, // Stack pointers and segment selectors
    ts_ss0: u16,       // after an increate in privilege level
    ts_padding1: u16,
    ts_esp1: VirtAddr,
    ts_ss1: u16,
    ts_padding2: u16,
    ts_esp2: VirtAddr,
    ts_ss2: u16,
    ts_padding3: u16,
    ts_cr3: PhysAddr, // Page directory base
    ts_eip: VirtAddr, // Saved state from last task switch
    ts_eflags: u32,
    ts_eax: u32, // More saved state (registers)
    ts_ecx: u32,
    ts_edx: u32,
    ts_ebx: u32,
    ts_esp: VirtAddr,
    ts_ebp: VirtAddr,
    ts_esi: u32,
    ts_edi: u32,
    ts_es: u16, // Even more saved state (segment selectors)
    ts_padding4: u16,
    ts_cs: u16,
    ts_padding5: u16,
    ts_ss: u16,
    ts_padding6: u16,
    ts_ds: u16,
    ts_padding7: u16,
    ts_fs: u16,
    ts_padding8: u16,
    ts_gs: u16,
    ts_padding9: u16,
    ts_ldt: u16,
    ts_padding10: u16,
    ts_t: u16,    // Trap on task switch
    ts_iomb: u16, // I/O map base address
}

impl TaskState {
    pub(crate) const fn new() -> TaskState {
        // Default trait looks useful here, but does not provide a const function.
        TaskState {
            ts_link: 0,
            ts_esp0: VirtAddr(0),
            ts_ss0: 0,
            ts_padding1: 0,
            ts_esp1: VirtAddr(0),
            ts_ss1: 0,
            ts_padding2: 0,
            ts_esp2: VirtAddr(0),
            ts_ss2: 0,
            ts_padding3: 0,
            ts_cr3: PhysAddr(0),
            ts_eip: VirtAddr(0),
            ts_eflags: 0,
            ts_eax: 0,
            ts_ecx: 0,
            ts_edx: 0,
            ts_ebx: 0,
            ts_esp: VirtAddr(0),
            ts_ebp: VirtAddr(0),
            ts_esi: 0,
            ts_edi: 0,
            ts_es: 0,
            ts_padding4: 0,
            ts_cs: 0,
            ts_padding5: 0,
            ts_ss: 0,
            ts_padding6: 0,
            ts_ds: 0,
            ts_padding7: 0,
            ts_fs: 0,
            ts_padding8: 0,
            ts_gs: 0,
            ts_padding9: 0,
            ts_ldt: 0,
            ts_padding10: 0,
            ts_t: 0,
            ts_iomb: 0,
        }
    }
}

/// A struct describing a pointer to a descriptor table (GDT / IDT).
/// This is in a format suitable for giving to 'lgdt' or 'lidt'.
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub(crate) struct DescriptorTablePointer {
    pub limit: u16, // Limit
    pub base: u32,  // Base address
}

/// Load GDT and segment descriptors.
pub(crate) fn init_percpu() {
    let gdt_pointer = DescriptorTablePointer {
        limit: (core::mem::size_of::<GlobalDescriptorTable>() - 1) as u16,
        base: VirtAddr(GDT.as_ptr() as u32).0,
    };
    x86::lgdt(&gdt_pointer);

    unsafe {
        // The kernel never uses GS or FS, so we leave those set to
        // the user data segment.
        asm! ("movw $0, %gs" : : "r" ((GDT_USER_DATA | 3) as u16) : "memory" : "volatile");
        asm! ("movw $0, %fs" : : "r" ((GDT_USER_DATA | 3) as u16) : "memory" : "volatile");

        // The kernel does use ES, DS, and SS.  We'll change between
        // the kernel and user data segments as needed.
        asm! ("movw $0, %es" : : "r" (GDT_KERNEL_DATA as u16) : "memory" : "volatile");
        asm! ("movw $0, %ds" : : "r" (GDT_KERNEL_DATA as u16) : "memory" : "volatile");
        asm! ("movw $0, %ss" : : "r" (GDT_KERNEL_DATA as u16) : "memory" : "volatile");

        // Load the kernel text segment into CS.
        // The second operand specifies a label defined in the next line.
        // The reason why a suffix 'f' is necessary is:
        // https://stackoverflow.com/questions/3898435/labels-in-gcc-inline-assembly
        // asm! ("ljmp $0, 1f; 1:" : : "i" (GDT_KERNEL_CODE) : : "volatile");
        asm!("push $0; \
              lea  1f, %eax; \
              push %eax; \
              lret; \
              1:" :: "i" (GDT_KERNEL_CODE) : "eax" "memory" : "volatile");
    }

    // For good measure, clear the local descriptor table (LDT),
    // since we don't use it.
    let null_ldt_pointer = DescriptorTablePointer { limit: 0, base: 0 };
    x86::lldt(&null_ldt_pointer);
}
