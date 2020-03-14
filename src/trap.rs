use crate::constants::*;
use crate::gdt::consts::*;
use crate::gdt::TaskState;
use crate::pmap::VirtAddr;
use crate::{env, gdt, sched, x86};
use crate::{lapic, mpconfig, syscall};
use consts::*;
use core::mem;
use core::slice;

static mut IDT: InterruptDescriptorTable = InterruptDescriptorTable([GateDesc::empty(); 256]);
static mut LAST_TF: Option<Trapframe> = None;

extern "C" {
    static vectors: u32;
}

pub(crate) mod consts {
    // Trap numbers
    // These are processor defined:
    pub(crate) const T_DIVIDE: u32 = 0; // divide error
    pub(crate) const T_DEBUG: u32 = 1; // debug exception
    pub(crate) const T_NMI: u32 = 2; // non-maskable interrupt
    pub(crate) const T_BRKPT: u32 = 3; // breakpoint
    pub(crate) const T_OFLOW: u32 = 4; // overflow
    pub(crate) const T_BOUND: u32 = 5; // bounds check
    pub(crate) const T_ILLOP: u32 = 6; // illega opcode
    pub(crate) const T_DEVICE: u32 = 7; // device not available
    pub(crate) const T_DBLFLT: u32 = 8; // double fault
    pub(crate) const T_COPROC: u32 = 9; // reserved (not generated by recent processors)
    pub(crate) const T_TSS: u32 = 10; // invalid task switch segment
    pub(crate) const T_SEGNP: u32 = 11; // segment not present
    pub(crate) const T_STACK: u32 = 12; // stack exception
    pub(crate) const T_GPFLT: u32 = 13; // general protection fault
    pub(crate) const T_PGFLT: u32 = 14; // page fault
    pub(crate) const T_RES: u32 = 15; // reserved
    pub(crate) const T_FPERR: u32 = 16; // floating point error
    pub(crate) const T_ALIGN: u32 = 17; // alignment check
    pub(crate) const T_MCHK: u32 = 18; // machine check
    pub(crate) const T_SIMDERR: u32 = 19; // SIMD floating point error
                                          // These are arbitrarily chosen, but with care not ot overlap
                                          // processor defined exceptions or interrupt vectors.
    pub(crate) const T_SYSCALL: u32 = 48; // system call
    pub(crate) const T_DEFAULT: u32 = 19; // catchall

    // System segment type bits
    pub(crate) const STS_IG32: u8 = 0xe; // 32-bit Interrupt Gate
    pub(crate) const STS_TG32: u8 = 0xf; // 32-bit Trap Gate

    // Hardware IRQ numbers. We receive these as (IRQ_OFFSET + IRQ_X)
    pub(crate) const IRQ_OFFSET: u8 = 32; // IRQ 0 corresponds to int IRQ_OFFSET

    pub(crate) const IRQ_TIMER: u8 = 0;
    pub(crate) const IRQ_KBD: u8 = 1;
    pub(crate) const IRQ_SERIAL: u8 = 4;
    pub(crate) const IRQ_SPURIOUS: u8 = 7;
    pub(crate) const IRQ_IDE: u8 = 14;
    pub(crate) const IRQ_ERROR: u8 = 19;
}

#[repr(align(4096))]
struct InterruptDescriptorTable([GateDesc; 256]);

// #[repr(C, packed)]
#[repr(C, align(8))]
struct GateDesc {
    offsetl: u16,
    selector: u16,
    count: u8,
    typ: u8,
    offseth: u16,
}

impl GateDesc {
    const fn empty() -> GateDesc {
        GateDesc {
            offsetl: 0,
            selector: 0,
            count: 0,
            typ: 0,
            offseth: 0,
        }
    }

    /// Set up a normal interrupt/trap gate descriptor.
    ///
    /// - istrap: 1 for a trap (= exception) gate, 0 for an interrupt gate.
    ///   see section 9.6.1.3 of the i386 reference: "The difference between
    ///   an interrupt gate and a trap gate is in the effect on IF (the
    ///   interrupt-enable flag). An interrupt that vectors through an
    ///   interrupt gate resets IF, thereby preventing other interrupts from
    ///   interfering with the current interrupt handler. A subsequent IRET
    ///   instruction restores IF to the value in the EFLAGS image on the
    ///   stack. An interrupt through a trap gate does not change IF."
    /// - sel: Code segment selector for interrupt/trap handler
    /// - off: Offset in code segment for interrupt/trap handler
    /// - dpl: Descriptor Privilege Level -
    ///	  the privilege level required for software to invoke
    ///	  this interrupt/trap gate explicitly using an int instruction.
    fn new(istrap: bool, sel: u16, off: u32, dpl: u8) -> GateDesc {
        let typ = if istrap { STS_TG32 } else { STS_IG32 };
        GateDesc {
            offsetl: (off & 0x0000ffff) as u16,
            selector: sel,
            count: 0,
            typ: typ | (dpl << 5) | (1 << 7), // typ | dpl | P (present)
            offseth: (off >> 16) as u16,
        }
    }
}

/// registers as pushed by pusha
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C, packed)]
pub(crate) struct PushRegs {
    pub(crate) reg_edi: u32,
    pub(crate) reg_esi: u32,
    pub(crate) reg_ebp: u32,
    pub(crate) reg_oesp: u32, // useless
    pub(crate) reg_ebx: u32,
    pub(crate) reg_edx: u32,
    pub(crate) reg_ecx: u32,
    pub(crate) reg_eax: u32,
}

impl PushRegs {
    fn new() -> PushRegs {
        PushRegs {
            reg_edi: 0,
            reg_esi: 0,
            reg_ebp: 0,
            reg_oesp: 0,
            reg_ebx: 0,
            reg_edx: 0,
            reg_ecx: 0,
            reg_eax: 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C, packed)]
pub(crate) struct Trapframe {
    pub(crate) tf_regs: PushRegs,
    pub(crate) tf_es: u16,
    pub(crate) tf_padding1: u16,
    pub(crate) tf_ds: u16,
    pub(crate) tf_padding2: u16,
    pub(crate) tf_trapno: u32,
    // below here defined by x86 hardware (see Intel SDM vol3. 6.12 EXCEPTION AND INTERRUPT HANDLING)
    pub(crate) tf_err: u32,
    pub(crate) tf_eip: usize,
    pub(crate) tf_cs: u16,
    pub(crate) tf_padding3: u16,
    pub(crate) tf_eflags: u32,
    // below here only when crossing rings, such as fron user to kernel (see Intel SDM vol3. 6.12 EXCEPTION AND INTERRUPT HANDLING)
    pub(crate) tf_esp: usize,
    pub(crate) tf_ss: u16,
    pub(crate) tf_padding4: u16,
}

impl Trapframe {
    pub(crate) fn new() -> Trapframe {
        Trapframe {
            tf_regs: PushRegs::new(),
            tf_es: 0,
            tf_padding1: 0,
            tf_ds: 0,
            tf_padding2: 0,
            tf_trapno: 0,
            tf_err: 0,
            tf_eip: 0,
            tf_cs: 0,
            tf_padding3: 0,
            tf_eflags: 0,
            tf_esp: 0,
            tf_ss: 0,
            tf_padding4: 0,
        }
    }

    /// Set up appropriate initial values for the segment registers.
    /// GD_UD is the user data segment selector in the GDT, and
    /// GD_UT is the user text segment selector (see inc/memlayout.h).
    /// The low 2 bits of each segment register contains the
    /// Requestor Privilege Level (RPL); 3 means user mode.  When
    /// we switch privilege levels, the hardware does various
    /// checks involving the RPL and the Descriptor Privilege Level
    /// (DPL) stored in the descriptors themselves.
    ///
    /// You have to set e->env_tf.tf_eip later.
    pub(crate) fn new_for_user() -> Trapframe {
        let mut tf = Trapframe::new();

        tf.tf_ds = GDT_USER_DATA | 3;
        tf.tf_es = GDT_USER_DATA | 3;
        tf.tf_ss = GDT_USER_DATA | 3;
        tf.tf_esp = USTACKTOP as usize;
        tf.tf_cs = GDT_USER_CODE | 3;

        tf.tf_eflags |= FL_IF;

        tf
    }

    pub(crate) fn set_entry_point(&mut self, va: VirtAddr) {
        self.tf_eip = va.0 as usize
    }
}

pub(crate) unsafe fn trap_init() {
    let vs = {
        let v = &vectors as *const u32;
        slice::from_raw_parts(v, 256)
    };

    IDT.0[0] = GateDesc::new(false, GDT_KERNEL_CODE, vs[0], 0);
    IDT.0[1] = GateDesc::new(false, GDT_KERNEL_CODE, vs[1], 0);
    IDT.0[2] = GateDesc::new(false, GDT_KERNEL_CODE, vs[2], 0);
    IDT.0[3] = GateDesc::new(true, GDT_KERNEL_CODE, vs[3], 3);
    IDT.0[4] = GateDesc::new(true, GDT_KERNEL_CODE, vs[4], 0);
    IDT.0[5] = GateDesc::new(false, GDT_KERNEL_CODE, vs[5], 0);
    IDT.0[6] = GateDesc::new(false, GDT_KERNEL_CODE, vs[6], 0);
    IDT.0[7] = GateDesc::new(false, GDT_KERNEL_CODE, vs[7], 0);
    IDT.0[8] = GateDesc::new(false, GDT_KERNEL_CODE, vs[8], 0);
    IDT.0[9] = GateDesc::new(false, GDT_KERNEL_CODE, vs[9], 0);
    IDT.0[10] = GateDesc::new(false, GDT_KERNEL_CODE, vs[10], 0);
    IDT.0[11] = GateDesc::new(false, GDT_KERNEL_CODE, vs[11], 0);
    IDT.0[12] = GateDesc::new(false, GDT_KERNEL_CODE, vs[12], 0);
    IDT.0[13] = GateDesc::new(false, GDT_KERNEL_CODE, vs[13], 0);
    IDT.0[14] = GateDesc::new(false, GDT_KERNEL_CODE, vs[14], 0);
    IDT.0[15] = GateDesc::new(false, GDT_KERNEL_CODE, vs[15], 0);
    IDT.0[16] = GateDesc::new(false, GDT_KERNEL_CODE, vs[16], 0);
    IDT.0[17] = GateDesc::new(false, GDT_KERNEL_CODE, vs[17], 0);
    IDT.0[18] = GateDesc::new(false, GDT_KERNEL_CODE, vs[18], 0);

    IDT.0[32] = GateDesc::new(false, GDT_KERNEL_CODE, vs[32], 0);
    IDT.0[33] = GateDesc::new(false, GDT_KERNEL_CODE, vs[33], 0);
    IDT.0[34] = GateDesc::new(false, GDT_KERNEL_CODE, vs[34], 0);
    IDT.0[35] = GateDesc::new(false, GDT_KERNEL_CODE, vs[35], 0);
    IDT.0[36] = GateDesc::new(false, GDT_KERNEL_CODE, vs[36], 0);
    IDT.0[37] = GateDesc::new(false, GDT_KERNEL_CODE, vs[37], 0);
    IDT.0[38] = GateDesc::new(false, GDT_KERNEL_CODE, vs[38], 0);
    IDT.0[39] = GateDesc::new(false, GDT_KERNEL_CODE, vs[39], 0);
    IDT.0[40] = GateDesc::new(false, GDT_KERNEL_CODE, vs[40], 0);
    IDT.0[41] = GateDesc::new(false, GDT_KERNEL_CODE, vs[41], 0);
    IDT.0[42] = GateDesc::new(false, GDT_KERNEL_CODE, vs[42], 0);
    IDT.0[43] = GateDesc::new(false, GDT_KERNEL_CODE, vs[43], 0);
    IDT.0[44] = GateDesc::new(false, GDT_KERNEL_CODE, vs[44], 0);
    IDT.0[45] = GateDesc::new(false, GDT_KERNEL_CODE, vs[45], 0);
    IDT.0[46] = GateDesc::new(false, GDT_KERNEL_CODE, vs[46], 0);
    IDT.0[47] = GateDesc::new(false, GDT_KERNEL_CODE, vs[47], 0);

    IDT.0[48] = GateDesc::new(false, GDT_KERNEL_CODE, vs[48], 3);

    trap_init_percpu();
}

/// Initialize and load the per-CPU TSS and IDT
pub(crate) unsafe fn trap_init_percpu() {
    // Setup a TSS so that we get the right stack
    // when we trap to the kernel.
    let cpu = mpconfig::this_cpu_mut();
    let selector = GDT_TSS0 + ((cpu.cpu_id as u16) << 3);

    let esp0 = VirtAddr(KSTACKTOP - (KSTKSIZE + KSTKGAP) * (cpu.cpu_id as u32));
    let ss0 = GDT_KERNEL_DATA;
    let iomb = mem::size_of::<TaskState>() as u16;
    let ts = cpu.init_ts(esp0, ss0, iomb);

    // Initialize the TSS slot of the gdt.
    gdt::set_tss(selector, ts);

    // Load the TSS selector (like other segment selectors,
    // the bottom three bits are special; we leave them 0)
    x86::ltr(selector);

    // Load the IDT
    let idt_pointer = gdt::DescriptorTablePointer {
        limit: (core::mem::size_of::<InterruptDescriptorTable>() - 1) as u16,
        base: VirtAddr(&IDT as *const InterruptDescriptorTable as u32).0,
    };
    x86::lidt(&idt_pointer);
}

fn trapname(trapno: u32) -> &'static str {
    match trapno {
        T_DIVIDE => "Divide error",
        T_DEBUG => "Debug",
        T_NMI => "Non-Maskable Interrupt",
        T_BRKPT => "Breakpoint",
        T_OFLOW => "Overflow",
        T_BOUND => "BOUND Rnage Exceeded",
        T_ILLOP => "Invalid Opcode",
        T_DEVICE => "Device Not Available",
        T_DBLFLT => "Double Fault",
        T_COPROC => "Coporocessor Segment Overrun",
        T_TSS => "Invalid TSS",
        T_SEGNP => "Segment Not Present",
        T_STACK => "Stack Fault",
        T_GPFLT => "General Protection",
        T_PGFLT => "Page Fault",
        T_RES => "(unknown trap)",
        T_FPERR => "x87 FPU Floating-Point Error",
        T_ALIGN => "Alignment Check",
        T_MCHK => "Machine-Check",
        T_SIMDERR => "SIMD Floating-Poitn Exception",
        T_SYSCALL => "System call",
        _ => "(unknown trap)",
    }
}

unsafe fn print_trapframe(tf: &Trapframe) {
    println!("TRAP frame at {:p}\n", tf);
    print_regs(&tf.tf_regs);
    println!("  es    0x----{:04x}", tf.tf_es);
    println!("  ds    0x----{:04x}", tf.tf_ds);
    println!("  trap  0x{:08x} {}", tf.tf_trapno, trapname(tf.tf_trapno));
    // If this trap was a page fault that just happened
    // (so %cr2 is meaningful), print the faulting linear address.
    if Some(tf) == LAST_TF.as_ref() && tf.tf_trapno == T_PGFLT {
        println!("  cr2   0x{:08x}", x86::rcr2());
    }
    print!("  err   0x{:08x}", tf.tf_err);
    // For page faults, print decoded fault error code:
    // U/K = fault occurred in user/kernel mode
    // W/R = a write/read caused the fault
    // PR = a protection violation caused the fault (NP = page not present)
    if tf.tf_trapno == T_PGFLT {
        println!(
            " [{}, {}, {}]",
            if tf.tf_err & 4 > 0 { "user" } else { "kernel" },
            if tf.tf_err & 2 > 0 { "write" } else { "read" },
            if tf.tf_err & 1 > 0 {
                "protection"
            } else {
                "not-present"
            }
        );
    } else {
        println!();
    }
    println!("  eip   0x{:08x}", tf.tf_eip);
    println!("  cs    0x----{:04x}", tf.tf_cs);
    println!("  flags 0x{:08x}", tf.tf_eflags);
    if tf.tf_cs & 3 > 0 {
        println!("  esp   0x{:08x}", tf.tf_esp);
        println!("  ss    0x----{:04x}", tf.tf_ss);
    }
}

unsafe fn print_regs(regs: &PushRegs) {
    println!("  edi   0x{:08x}", regs.reg_edi);
    println!("  esi   0x{:08x}", regs.reg_esi);
    println!("  ebp   0x{:08x}", regs.reg_ebp);
    println!("  oesp  0x{:08x}", regs.reg_oesp);
    println!("  ebx   0x{:08x}", regs.reg_ebx);
    println!("  edx   0x{:08x}", regs.reg_edx);
    println!("  ecx   0x{:08x}", regs.reg_ecx);
    println!("  eax   0x{:08x}", regs.reg_eax);
}

fn trap_dispatch(tf: &mut Trapframe) {
    // Handle processor exceptions.
    if tf.tf_trapno == (IRQ_OFFSET + IRQ_TIMER) as u32 {
        lapic::eoi();
        sched::sched_yield();
    } else if tf.tf_trapno == T_SYSCALL {
        unsafe {
            let ret = syscall::syscall(
                tf.tf_regs.reg_eax,
                tf.tf_regs.reg_edx,
                tf.tf_regs.reg_ecx,
                tf.tf_regs.reg_ebx,
                tf.tf_regs.reg_edi,
                tf.tf_regs.reg_esi,
            );
            tf.tf_regs.reg_eax = ret as u32;
        }
    } else {
        // Unexpected trap: The user process or the kernel has a bug.
        unsafe {
            print_trapframe(tf);
        }
        if tf.tf_cs == GDT_KERNEL_CODE {
            panic!("unhandled trap in kernel")
        } else {
            let curenv = env::cur_env_mut().expect("there is no running Env");
            let env_table = env::env_table();
            env::env_destroy(curenv.get_env_id(), env_table);
        }
    }
}

#[no_mangle]
extern "C" fn trap(orig_tf: *mut Trapframe) -> ! {
    let mut tf = unsafe { orig_tf.as_mut().unwrap() };

    // The environment may have set DF and some versions
    // of GCC rely on DF being clear
    x86::cld();

    // Check that interrupts are disabled.  If this assertion
    // fails, DO NOT be tempted to fix it by inserting a "cli" in
    // the interrupt path.
    assert_eq!(
        x86::read_eflags() & FL_IF,
        0x0,
        "interrupts should be disabled"
    );

    println!("Incoming TRAP frame at {:?}", tf as *const Trapframe);

    // Trapped from user mode
    if tf.tf_cs & 3 == 3 {
        let curenv = env::cur_env_mut().expect("there is no running Env");

        if curenv.is_dying() {
            let env_table = env::env_table();
            env::env_destroy(curenv.get_env_id(), env_table);
        }

        // Copy trap frame (which is currently on the stack)
        // into 'curenv->env_tf', so that running the environment
        // will restart at the trap point.
        curenv.set_tf(tf);

        // The trapframe on the stack should be ignored from here on.
        tf = curenv.get_tf_mut();
    }

    // Record that tf is the last real trapframe so
    // print_trapframe can print some additional information.
    unsafe {
        LAST_TF = Some(tf.clone());
    }

    // Dispatch based on what type of trap occurred
    trap_dispatch(tf);

    // Return to the current environment, which should be running.
    {
        let curenv = env::cur_env_mut().expect("there is no running Env");
        assert!(curenv.is_running(), "the Env is not running");
        let env_id = curenv.get_env_id();
        let table = env::env_table();
        env::env_run(env_id, table);
    }
}