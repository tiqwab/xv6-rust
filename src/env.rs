use alloc::boxed::Box;
use core::ptr::{null, null_mut};

use crate::constants::*;
use crate::elf::{Elf, ElfParser, Proghdr, ProghdrType};
use crate::pmap::{PageDirectory, PhysAddr, VirtAddr, PDX};
use crate::spinlock::{Mutex, MutexGuard};
use crate::trap::Trapframe;
use crate::{file, fs, log, mpconfig, pmap, sched, util, x86};
use core::fmt::{Error, Formatter};
use core::{cmp, fmt, mem};

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
    Zombie,
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
    env_cwd: Arc<RwLock<Inode>>,   // Current working directory
    env_ofile: [Option<FileTableEntry>; NFILE_PER_ENV], // Open files
    env_heap_size: usize,          // allocated user heap size
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

    pub(crate) fn is_zombie(&self) -> bool {
        self.env_status == EnvStatus::Zombie
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

    pub(crate) fn get_pgdir_paddr(&mut self) -> PhysAddr {
        self.env_pgdir.paddr().unwrap()
    }

    pub(crate) fn get_cwd(&self) -> &Arc<RwLock<Inode>> {
        &self.env_cwd
    }

    pub(crate) fn change_cwd(&mut self, ip: &Arc<RwLock<Inode>>) {
        let old = Arc::clone(&self.env_cwd);
        fs::iput(old);
        self.env_cwd = Arc::clone(ip);
    }

    pub(crate) fn fd_alloc(
        &mut self,
        ent: FileTableEntry,
    ) -> Result<FileDescriptor, FileTableEntry> {
        for (fd, ent_opt) in self.env_ofile.iter_mut().enumerate() {
            if ent_opt.is_none() {
                *ent_opt = Some(ent);
                return Ok(FileDescriptor(fd as u32));
            }
        }
        Err(ent)
    }

    pub(crate) fn fd_close(&mut self, fd: FileDescriptor) -> FileTableEntry {
        assert!(
            (fd.0 as usize) >= 0 && (fd.0 as usize) < self.env_ofile.len(),
            "illegal fd"
        );
        let ent = self.env_ofile[fd.0 as usize].take();
        ent.expect("illegal fd")
    }

    pub(crate) fn fd_get(&mut self, fd: FileDescriptor) -> Option<&mut FileTableEntry> {
        self.env_ofile
            .get_mut(fd.0 as usize)
            .and_then(|ent_opt| ent_opt.as_mut())
    }

    pub(crate) fn fd_dup(&mut self, fd: FileDescriptor) -> Option<FileDescriptor> {
        let ent_opt = self.fd_get(fd).map(|ent| ent.clone());

        ent_opt.and_then(|ent| {
            let mut res = None;
            for (fd, ent_opt) in self.env_ofile.iter_mut().enumerate() {
                if ent_opt.is_none() {
                    *ent_opt = Some(ent.clone());
                    res = Some(FileDescriptor(fd as u32));
                    break;
                }
            }
            res
        })
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

    pub(crate) fn find(&self, env_id: EnvId) -> Option<&Env> {
        for env_opt in self.envs.iter() {
            if let Some(env) = env_opt {
                if env.get_env_id() == env_id {
                    return Some(env);
                }
            }
        }
        None
    }

    pub(crate) fn find_mut(&mut self, env_id: EnvId) -> Option<&mut Env> {
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
    fn env_alloc(&mut self, parent_id: EnvId, typ: EnvType, cwd: Arc<RwLock<Inode>>) -> EnvId {
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
            env_cwd: cwd,
            env_ofile: [None; NFILE_PER_ENV],
            env_heap_size: 0,
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
        let stack_size = USTACKSIZE as usize;
        env.env_pgdir.region_alloc(stack_base, stack_size);

        // Restore kern page directory
        x86::lcr3(kern_pgdir);

        // Set trapframe
        env.set_entry_point(elf.entry_point());
    }

    /// Frees resources and memory the env uses except for the entry of env_table.
    /// Use wait_env_id to release the entry.
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

        // Close all file descriptors
        for ent_opt in env.env_ofile.iter_mut() {
            let ent_opt = ent_opt.take();
            match ent_opt {
                None => (),
                Some(ent) => {
                    file::file_table().close(ent);
                }
            }
        }

        // Change the state to zombie.
        // Call wait_env_id to release the entry later.
        env.env_status = EnvStatus::Zombie;
    }

    /// Release the entry of EnvTable.
    /// Parent process uses this when it waits child process.
    fn env_release(&mut self, env_id: EnvId) -> Option<EnvId> {
        let child_opt = self.find(env_id).and_then(|child| {
            if !child.is_zombie() {
                None
            } else {
                Some(child)
            }
        });

        match child_opt {
            None => None,
            Some(_) => {
                let idx = self.get_idx(env_id).unwrap();
                self.envs[idx] = None;
                Some(env_id)
            }
        }
    }

    /// Create a new process copying p as the parent.
    /// Sets up stack to return as if from system call.
    /// Caller must set state of returned proc to RUNNABLE.
    ///
    /// ref. fork() in proc.c (xv6)
    fn fork(&mut self, parent: &mut Env) -> EnvId {
        let root_inode = fs::iget(ROOT_DEV, ROOT_INUM);

        // Allocate process.
        let new_env_id = self.env_alloc(parent.env_id, EnvType::User, root_inode);
        let new_env = self.find_mut(new_env_id).unwrap();

        // Copy process state from parent.
        new_env.env_pgdir.copy_uvm(&mut parent.env_pgdir);

        new_env.env_tf = parent.env_tf;

        // Clear %eax so that fork returns 0 in the child.
        new_env.env_tf.tf_regs.reg_eax = 0;

        // Dup env_ofile
        for (i, ent_opt) in parent.env_ofile.iter().enumerate() {
            new_env.env_ofile[i] = ent_opt.clone();
        }

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

use crate::file::{File, FileDescriptor, FileTableEntry};
use crate::fs::Inode;
use crate::rwlock::RwLock;
use alloc::sync::Arc;
use core::ops::Add;

/// Allocates a new env with env_alloc, loads the named elf
/// binary into it with load_icode, and sets its env_type.
/// This function is ONLY called during kernel initialization,
/// before running the first user-mode environment.
/// The new env's parent ID is set to 0.
pub(crate) fn env_create_for_init(env_table: &mut EnvTable) -> EnvId {
    extern "C" {
        static _binary_obj_user_init_start: u8;
        static _binary_obj_user_init_end: u8;
        static _binary_obj_user_init_size: usize;
    }

    let root_inode = crate::fs::iget(ROOT_DEV, ROOT_INUM);
    let env_id = env_table.env_alloc(EnvId(0), EnvType::User, root_inode);

    unsafe {
        let user_init_start = &_binary_obj_user_init_start as *const u8;
        let _user_init_end = &_binary_obj_user_init_end as *const u8;
        let _user_init_size = &_binary_obj_user_init_size as *const usize;

        env_table.load_icode(env_id, user_init_start);
    }

    env_id
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

fn load_from_disk(mut dst: VirtAddr, inode: &mut Inode, mut off: u32, mut remain_sz: u32) {
    while remain_sz > 0 {
        let sz = cmp::min(PGSIZE, remain_sz);
        if fs::readi(inode, dst.as_mut_ptr(), off, sz) != sz {
            panic!("load_from_disk: failed to readi");
        }
        dst += sz;
        off += sz;
        remain_sz -= sz;
    }
}

pub(crate) fn exec(path: *const u8, argv: &[*const u8], env: &mut Env) {
    // Allocate and set up the page directory for this environment.
    let new_pgdir = env_setup_vm();
    env.env_pgdir = new_pgdir;

    // Change page directory to that of env temporally
    x86::lcr3(env.get_pgdir_paddr());

    log::begin_op();

    let ip = fs::namei(path).unwrap();

    let mut inode = fs::ilock(&ip);

    // Read ELF header
    let mut buf_elf = [0 as u8; mem::size_of::<Elf>()];
    let elf = unsafe { &*(buf_elf.as_ptr() as *const Elf) };
    if fs::readi(
        &mut inode,
        buf_elf.as_mut_ptr(),
        0,
        mem::size_of::<Elf>() as u32,
    ) != mem::size_of::<Elf>() as u32
    {
        panic!("exec: failed to read elf header")
    }
    if !elf.is_valid() {
        panic!("exec: illgal elf header");
    }

    let mut buf_ph = [0 as u8; mem::size_of::<Proghdr>()];
    let ph = unsafe { &*(buf_ph.as_ptr() as *const Proghdr) };

    // Read program header and set up memory
    for i in 0..elf.e_phnum {
        let bs = {
            let off = elf.e_phoff + (mem::size_of::<Proghdr>() as u32) * (i as u32);
            if fs::readi(
                &mut inode,
                buf_ph.as_mut_ptr(),
                off,
                mem::size_of::<Proghdr>() as u32,
            ) != mem::size_of::<Proghdr>() as u32
            {
                panic!("exec: failed to read program header");
            }
        };

        if ph.p_type != ProghdrType::PtLoad {
            continue;
        }

        let dest_va = VirtAddr(ph.p_vaddr);
        let memsz = ph.p_memsz as usize;
        let filesz = ph.p_filesz as usize;

        // Allocation necessary memory
        env.env_pgdir.as_mut().region_alloc(dest_va, memsz);

        // Load data from disk (and occupy zero)
        unsafe {
            load_from_disk(dest_va, &mut inode, ph.p_offset, filesz as u32);
            // util::memcpy(dest_va, src_va, filesz);
            util::memset(dest_va + filesz, 0, memsz - filesz);
        }
    }

    fs::iunlock(inode);
    log::end_op();

    // Now map one page for the program's initial stack
    // at virtual address USTACKTOP - PGSIZE.
    let stack_base = VirtAddr(USTACKTOP - USTACKSIZE);
    let stack_size = USTACKSIZE as usize;
    env.env_pgdir.region_alloc(stack_base, stack_size);

    // Prepare args
    let mut sp: *mut u8 = stack_base.add(stack_size).as_mut_ptr();
    unsafe {
        let mut ustack = [0 as u32; 3 + MAX_CMD_ARGS]; // +3 is for return address, argv, and argc
        for (i, s) in argv.iter().enumerate() {
            let len = util::strnlen(*s, MAX_CMD_ARG_LEN);
            sp = sp.sub(len + 1);
            util::strncpy(sp, *s, len + 1);
            ustack[3 + i] = sp as u32;
        }
        sp = sp.sub(mem::size_of_val(&ustack));
        sp = VirtAddr(sp as u32).round_down(4).as_mut_ptr();
        util::memcpy(
            VirtAddr(sp as u32),
            VirtAddr(&ustack as *const _ as u32),
            mem::size_of_val(&ustack),
        );
        *sp.cast::<u32>() = argv.len() as u32; // argc
        *sp.add(4).cast::<u32>() = sp.add(12) as u32; // argv
    }

    // Set up appropriate initial values for the segment registers.
    // You will set e->env_tf.tf_eip later.
    let new_tf = Trapframe::new_for_user();
    env.env_tf = new_tf;
    env.env_tf.tf_esp = sp as usize;

    // Set trapframe
    env.set_entry_point(elf.entry_point());

    // TODO: is there any other things to do here?
}

pub(crate) fn wait_env_id(env_id: EnvId) -> Option<EnvId> {
    let mut env_table = env_table();
    env_table.env_release(env_id)
}

/// Allocate user heap.
/// Assume that the initial break is UHEAPBASE.
pub(crate) fn sbrk(nbytes: usize) -> *const u8 {
    let env = cur_env_mut().unwrap();
    let pgdir = &mut env.env_pgdir;

    // round up by PGSIZE
    let required_size = {
        let pgsize = PGSIZE as usize;
        (nbytes + pgsize - 1) / pgsize * pgsize
    };

    if env.env_heap_size + required_size > UHEAPSIZE {
        return null();
    }

    let cur_heap_top = VirtAddr(UHEAPBASE + (env.env_heap_size as u32));
    pgdir.region_alloc(cur_heap_top, required_size);
    env.env_heap_size += required_size;

    cur_heap_top.as_ptr::<u8>()
}
