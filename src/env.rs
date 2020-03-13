use alloc::boxed::Box;
use core::ptr::{null, null_mut};

use crate::constants::*;
use crate::elf::{ElfParser, ProghdrType};
use crate::pmap::{PageDirectory, VirtAddr, PDX};
use crate::spinlock::{Mutex, MutexGuard};
use crate::trap::Trapframe;
use crate::{mpconfig, pmap, sched, util, x86};
use core::fmt;
use core::fmt::{Error, Formatter};

const LOG2ENV: u32 = 10;
const NENV: u32 = 1 << LOG2ENV;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) struct EnvId(pub(crate) u32);

impl fmt::LowerHex for EnvId {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        let val = self.0;
        fmt::LowerHex::fmt(&val, f)
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum EnvType {
    User,
}

#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)]
enum EnvStatus {
    Free,
    Dying,
    Runnable,
    Running,
    NotRunnable,
}

#[repr(C)]
pub(crate) struct Env {
    env_tf: Trapframe,     // Saved registers
    env_id: EnvId,         // Unique environment identifier
    env_parent_id: EnvId,  // env_id of this env's parent
    env_type: EnvType,     // Indicates special system environments
    env_status: EnvStatus, // Status of the environment
    env_runs: u32,         // Number of times environment has run
    // FIXME: what type is better for env_pgdir?
    env_pgdir: Box<PageDirectory>, // Kernel virtual address of page dir
}

impl PartialEq for Env {
    fn eq(&self, other: &Self) -> bool {
        self.env_id == other.env_id
    }
}

impl Eq for Env {}

impl Env {
    fn set_entry_point(&mut self, va: VirtAddr) {
        self.env_tf.set_entry_point(va);
    }

    pub(crate) fn is_running(&self) -> bool {
        self.env_status == EnvStatus::Running
    }

    pub(crate) fn is_runnable(&self) -> bool {
        self.env_status == EnvStatus::Runnable
    }

    pub(crate) fn is_dying(&self) -> bool {
        self.env_status == EnvStatus::Dying
    }

    fn pause(&mut self) {
        self.env_status = EnvStatus::Runnable;
    }

    fn resume(&mut self) {
        self.env_status = EnvStatus::Running;
        self.env_runs += 1;
    }

    fn die(&mut self) {
        self.env_status = EnvStatus::Dying;
    }

    pub(crate) fn get_tf(&self) -> &Trapframe {
        &self.env_tf
    }

    pub(crate) fn get_tf_mut(&mut self) -> &mut Trapframe {
        &mut self.env_tf
    }

    pub(crate) fn set_tf(&mut self, tf: &Trapframe) {
        self.env_tf = tf.clone();
    }

    pub(crate) fn get_env_id(&self) -> EnvId {
        self.env_id
    }
}

pub(crate) struct EnvTable {
    envs: [Option<Env>; NENV as usize],
    next_env_id: u32,
}

impl EnvTable {
    fn generate_env_id(&mut self) -> EnvId {
        let res = self.next_env_id;
        self.next_env_id += 1;
        EnvId(res)
    }

    fn find(&self, env_id: EnvId) -> Option<&Env> {
        for env_opt in self.envs.iter() {
            if let Some(env) = env_opt {
                if env.get_env_id() == env_id {
                    return Some(env);
                }
            }
        }
        None
    }

    fn find_mut(&mut self, env_id: EnvId) -> Option<&mut Env> {
        for env_opt in &mut self.envs.iter_mut() {
            if let Some(env) = env_opt {
                if env.get_env_id() == env_id {
                    return Some(env);
                }
            }
        }
        None
    }

    fn get_idx(&mut self, env_id: EnvId) -> Option<usize> {
        for (i, env_opt) in &mut self.envs.iter().enumerate() {
            if let Some(env) = env_opt {
                if env.get_env_id() == env_id {
                    return Some(i);
                }
            }
        }
        None
    }

    pub(crate) fn find_runnable(&mut self) -> Option<EnvId> {
        let start = cur_env()
            .and_then(|e| self.get_idx(e.get_env_id()))
            .map(|idx| idx + 1)
            .unwrap_or(0) as usize;
        for i in 0..(NENV as usize) {
            let idx = (start + i) % (NENV as usize);
            if let Some(env) = &mut self.envs[idx] {
                if env.is_runnable() {
                    return Some(env.get_env_id());
                }
            }
        }

        // This is the case where only the current env is runnable (actually it is running)
        if start > 0 {
            if let Some(env) = &mut self.envs[start - 1] {
                return Some(env.get_env_id());
            }
        }

        None
    }

    /// Allocates and initializes a new environment.
    /// On success, the new environment is stored in *newenv_store.
    ///
    /// Returns 0 on success, < 0 on failure.  Errors include:
    ///	-E_NO_FREE_ENV if all NENV environments are allocated
    ///	-E_NO_MEM on memory exhaustion
    fn env_alloc(&mut self, parent_id: EnvId, typ: EnvType) -> EnvId {
        let mut idx = -1;
        for (i, env_opt) in self.envs.iter().enumerate() {
            if env_opt.is_none() {
                idx = i as i32;
                break;
            }
        }
        if idx == -1 {
            panic!("no available env");
        }

        // Allocate and set up the page directory for this environment.
        let new_pgdir = env_setup_vm();

        // Generate an env_id for this environment.
        let new_id = self.generate_env_id();

        // Set up appropriate initial values for the segment registers.
        // You will set e->env_tf.tf_eip later.
        let new_tf = Trapframe::new_for_user();

        let new_env = Env {
            env_tf: new_tf,
            env_id: new_id,
            env_parent_id: parent_id,
            env_type: typ,
            env_status: EnvStatus::Runnable,
            env_runs: 0,
            env_pgdir: new_pgdir,
        };

        let env_opt = &mut self.envs[idx as usize];
        *env_opt = Some(new_env);

        new_id
    }

    /// Set up the initial program binary, stack, and processor flags
    /// for a user process.
    /// This function is ONLY called during kernel initialization,
    /// before running the first user-mode environment.
    ///
    /// This function loads all loadable segments from the ELF binary image
    /// into the environment's user memory, starting at the appropriate
    /// virtual addresses indicated in the ELF program header.
    /// At the same time it clears to zero any portions of these segments
    /// that are marked in the program header as being mapped
    /// but not actually present in the ELF file - i.e., the program's bss section.
    ///
    /// All this is very similar to what our boot loader does, except the boot
    /// loader also needs to read the code from disk.  Take a look at
    /// boot/main.c to get ideas.
    ///
    /// Finally, this function maps one page for the program's initial stack.
    unsafe fn load_icode(&mut self, env_id: EnvId, binary: *const u8) {
        let env = self.find_mut(env_id).expect("illegal env_id");

        let elf = ElfParser::new(binary).expect("binary is not elf");

        // Change page directory to that of env temporally
        let kern_pgdir = x86::rcr3();
        x86::lcr3(
            env.env_pgdir
                .paddr()
                .expect("failed to get a paddr of pgdir"),
        );

        for ph in elf.program_headers() {
            if ph.p_type != ProghdrType::PtLoad {
                continue;
            }

            let src_va = VirtAddr(binary as u32 + ph.p_offset);
            let dest_va = VirtAddr(ph.p_vaddr);
            let memsz = ph.p_memsz as usize;
            let filesz = ph.p_filesz as usize;

            env.env_pgdir
                .as_mut()
                .region_alloc(dest_va, ph.p_memsz as usize);

            util::memcpy(dest_va, src_va, filesz);
            util::memset(dest_va + filesz, 0, memsz - filesz);
        }

        // Now map one page for the program's initial stack
        // at virtual address USTACKTOP - PGSIZE.
        let stack_base = VirtAddr(USTACKTOP - PGSIZE);
        let stack_size = PGSIZE as usize;
        env.env_pgdir.region_alloc(stack_base, stack_size);

        // Restore kern page directory
        x86::lcr3(kern_pgdir);

        // Set trapframe
        env.set_entry_point(elf.entry_point());
    }

    /// Frees env and all memory it uses.
    unsafe fn env_free(&mut self, env_id: EnvId) {
        let env = self.find_mut(env_id).expect("illegal env_id");

        // If freeing the current environment, switch to kern_pgdir
        // before freeing the page directory, just in case the page
        // gets reused.
        match cur_env_mut() {
            Some(e) if e.env_id == env.env_id => {
                pmap::load_kern_pgdir();
            }
            _ => {}
        }

        // Note the environment's demise.
        {
            let curenv_id = cur_env().map(Env::get_env_id).map(|x| x.0).unwrap_or(0);
            println!("[{:08x}] free env {:08x}", curenv_id, env.env_id);
        }

        // Flush all mapped pages in the user portion of the address space
        assert_eq!(UTOP % (PTSIZE as u32), 0);
        let start_pdx = PDX::new(VirtAddr(0));
        let end_pdx = PDX::new(VirtAddr(UTOP));
        let mut pdx = start_pdx;
        while pdx < end_pdx {
            let pde = &env.env_pgdir[pdx];
            // only look at mapped page tables
            if pde.exists() {
                // unmap all PTEs in this page table
                env.env_pgdir.remove_pde(pdx);
            }
            pdx += 1;
        }

        // free the page directory
        // The allocation of pgdir is currently managed by rust,
        // so do nothing here

        // return the environment to the free list
        env.env_status = EnvStatus::Free;

        for entry_opt in self.envs.iter_mut() {
            match entry_opt {
                Some(entry) if entry.env_id == env_id => {
                    *entry_opt = None;
                }
                _ => (),
            }
        }
    }

    /// Create a new process copying p as the parent.
    /// Sets up stack to return as if from system call.
    /// Caller must set state of returned proc to RUNNABLE.
    ///
    /// ref. fork() in proc.c (xv6)
    fn fork(&mut self, parent: &mut Env) -> EnvId {
        // Allocate process.
        let new_env_id = self.env_alloc(parent.env_id, EnvType::User);
        let new_env = self.find_mut(new_env_id).unwrap();

        // Copy process state from parent.
        new_env.env_pgdir.copy_uvm(&mut parent.env_pgdir);

        new_env.env_tf = parent.env_tf;

        // Clear %eax so that fork returns 0 in the child.
        new_env.env_tf.tf_regs.reg_eax = 0;

        // TODO: Duplicate open file descriptors here.

        new_env_id
    }
}

static ENV_TABLE: Mutex<EnvTable> = Mutex::new(EnvTable {
    envs: [None; NENV as usize],
    next_env_id: 1,
});

pub(crate) fn env_table() -> MutexGuard<'static, EnvTable> {
    ENV_TABLE.lock()
}

pub(crate) fn cur_env() -> Option<&'static Env> {
    mpconfig::this_cpu().cur_env()
}

pub(crate) fn cur_env_mut() -> Option<&'static mut Env> {
    mpconfig::this_cpu_mut().cur_env_mut()
}

// Initialize the kernel virtual memory layout for environment e.
// Allocate a page directory, set e->env_pgdir accordingly,
// and initialize the kernel portion of the new environment's address space.
// Do NOT (yet) map anything into the user portion
// of the environment's virtual address space.
//
// Returns 0 on success, < 0 on error.  Errors include:
//	-E_NO_MEM if page directory or table could not be allocated.
fn env_setup_vm() -> Box<PageDirectory> {
    PageDirectory::new_for_user()
}

pub(crate) use temporary::*;

mod temporary {
    use crate::env::*;

    /// Allocates a new env with env_alloc, loads the named elf
    /// binary into it with load_icode, and sets its env_type.
    /// This function is ONLY called during kernel initialization,
    /// before running the first user-mode environment.
    /// The new env's parent ID is set to 0.
    pub(crate) fn env_create_for_hello(env_table: &mut EnvTable) -> EnvId {
        extern "C" {
            static _binary_obj_user_hello_start: u8;
            static _binary_obj_user_hello_end: u8;
            static _binary_obj_user_hello_size: usize;
        }

        let env_id = env_table.env_alloc(EnvId(0), EnvType::User);

        unsafe {
            let user_hello_start = &_binary_obj_user_hello_start as *const u8;
            let user_hello_end = &_binary_obj_user_hello_end as *const u8;
            let user_hello_size = &_binary_obj_user_hello_size as *const usize;

            println!("_binary_obj_user_hello_start: {:?}", user_hello_start);
            println!("_binary_obj_user_hello_end: {:?}", user_hello_end);
            println!("_binary_obj_user_hello_size: {:?}", user_hello_size);

            env_table.load_icode(env_id, user_hello_start);
        }

        env_id
    }

    pub(crate) fn env_create_for_yield(env_table: &mut EnvTable) -> EnvId {
        extern "C" {
            static _binary_obj_user_yield_start: u8;
            static _binary_obj_user_yield_end: u8;
            static _binary_obj_user_yield_size: usize;
        }

        let env_id = env_table.env_alloc(EnvId(0), EnvType::User);

        unsafe {
            let user_yield_start = &_binary_obj_user_yield_start as *const u8;
            let _user_yield_end = &_binary_obj_user_yield_end as *const u8;
            let _user_yield_size = &_binary_obj_user_yield_size as *const usize;

            env_table.load_icode(env_id, user_yield_start);
        }

        env_id
    }

    pub(crate) fn env_create_for_forktest(env_table: &mut EnvTable) -> EnvId {
        extern "C" {
            static _binary_obj_user_forktest_start: u8;
            static _binary_obj_user_forktest_end: u8;
            static _binary_obj_user_forktest_size: usize;
        }

        let env_id = env_table.env_alloc(EnvId(0), EnvType::User);

        unsafe {
            let user_forktest_start = &_binary_obj_user_forktest_start as *const u8;
            let _user_forktest_end = &_binary_obj_user_forktest_end as *const u8;
            let _user_forktest_size = &_binary_obj_user_forktest_size as *const usize;

            env_table.load_icode(env_id, user_forktest_start);
        }

        env_id
    }

    pub(crate) fn env_create_for_spin(env_table: &mut EnvTable) -> EnvId {
        extern "C" {
            static _binary_obj_user_spin_start: u8;
            static _binary_obj_user_spin_end: u8;
            static _binary_obj_user_spin_size: usize;
        }

        let env_id = env_table.env_alloc(EnvId(0), EnvType::User);

        unsafe {
            let user_spin_start = &_binary_obj_user_spin_start as *const u8;
            let _user_spin_end = &_binary_obj_user_spin_end as *const u8;
            let _user_spin_size = &_binary_obj_user_spin_size as *const usize;

            env_table.load_icode(env_id, user_spin_start);
        }

        env_id
    }
}

/// Restores the register values in the Trapframe with the 'iret' instruction.
/// This exits the kernel and starts executing some environment's code.
///
/// This function does not return.
fn env_pop_tf(tf: *const Trapframe) -> ! {
    unsafe {
        asm!(
        "movl $0, %esp; \
        popal; \
        popl %es; \
        popl %ds; \
        addl $1, %esp; \
        iret"
        : : "rmi" (tf), "i" (0x8) : "memory" : "volatile"
        );
    }

    panic!("iret failed")
}

/// Context switch from curenv to env e.
/// Note: if this is the first call to env_run, curenv is NULL.
/// Note: This function unlock a passed MutexGuard<ENV_TABLE>.
///
/// This function does not return.
pub(crate) fn env_run(env_id: EnvId, mut table: MutexGuard<EnvTable>) -> ! {
    if let Some(cur) = cur_env_mut().filter(|e| e.is_running()) {
        cur.pause();
    }

    let env = (*table).find_mut(env_id).unwrap();
    let env_tf = &env.env_tf as *const Trapframe;

    env.resume();
    mpconfig::this_cpu_mut().set_env(env);
    x86::lcr3(env.env_pgdir.paddr().unwrap());

    // Unlock EnvTable
    drop(table);

    env_pop_tf(env_tf);
}

/// Frees an environment.
///
/// If env was the current env, then runs a new environment (and does not
/// return to the caller).
pub(crate) fn env_destroy(env_id: EnvId, mut env_table: MutexGuard<EnvTable>) {
    let env = env_table.find_mut(env_id).expect("illegal env_id");

    let is_myself = if let Some(cur_env) = cur_env() {
        cur_env.get_env_id() == env.get_env_id()
    } else {
        false
    };

    // If e is currently running on other CPUs, we change its state to
    // ENV_DYING. A zombie environment will be freed the next time
    // it traps to the kernel.
    if env.is_running() && !is_myself {
        env.die();
    } else {
        unsafe { env_table.env_free(env_id) };

        if is_myself {
            mpconfig::this_cpu_mut().unset_env();
            drop(env_table);
            sched::sched_yield();
        }
    }
}

/// Checks that environment 'env' is allowed to access the range
/// of memory [va, va+len) with permissions 'perm | PTE_U | PTE_P'.
/// If it can, then the function simply returns.
/// If it cannot, 'env' is destroyed and, if env is the current
/// environment, this function will not return.
pub(crate) fn user_mem_assert(env: &mut Env, va: VirtAddr, len: usize, perm: u32) {
    if let Err(addr) = env.env_pgdir.user_mem_check(va, len, perm | PTE_U) {
        println!(
            "[{:08x}] user_mem_check assertion failure for va {:08x}",
            env.env_id, addr.0
        );

        let env_table = env_table();
        env_destroy(env.get_env_id(), env_table);
    }
}

pub(crate) fn fork(parent: &mut Env) -> EnvId {
    let mut env_table = env_table();
    env_table.fork(parent)
}
