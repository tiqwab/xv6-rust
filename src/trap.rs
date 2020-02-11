use crate::constants::*;
use crate::gdt::consts::*;
use crate::pmap::VirtAddr;

/// registers as pushed by pusha
#[repr(C, packed)]
struct PushRegs {
    reg_edi: u32,
    reg_esi: u32,
    reg_ebp: u32,
    reg_oesp: u32, // useless
    reg_ebx: u32,
    reg_edx: u32,
    reg_ecx: u32,
    reg_eax: u32,
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

#[repr(C, packed)]
pub(crate) struct Trapframe {
    tf_regs: PushRegs,
    tf_es: u16,
    tf_padding1: u16,
    tf_ds: u16,
    tf_padding2: u16,
    tf_trapno: u32,
    // below here defined by x86 hardware (see Intel SDM vol3. 6.12 EXCEPTION AND INTERRUPT HANDLING)
    tf_err: u32,
    tf_eip: usize,
    tf_cs: u16,
    tf_padding3: u16,
    tf_eflags: u32,
    // below here only when crossing rings, such as fron user to kernel (see Intel SDM vol3. 6.12 EXCEPTION AND INTERRUPT HANDLING)
    tf_esp: usize,
    tf_ss: u16,
    tf_padding4: u16,
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

        tf.tf_ds = (GDT_USER_DATA | 3) as u16;
        tf.tf_es = (GDT_USER_DATA | 3) as u16;
        tf.tf_ss = (GDT_USER_DATA | 3) as u16;
        tf.tf_esp = USTACKTOP as usize;
        tf.tf_cs = (GDT_USER_CODE | 3) as u16;

        tf
    }

    pub(crate) fn set_entry_point(&mut self, va: VirtAddr) {
        self.tf_eip = va.0 as usize
    }
}
