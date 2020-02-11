use alloc::boxed::Box;
use core::ptr::{null, null_mut};

use crate::constants::*;
use crate::elf::{ElfParser, ProghdrType};
use crate::pmap::{PageDirectory, VirtAddr};
use crate::trap::Trapframe;
use crate::{util, x86};

extern "C" {
    static _binary_obj_user_nop_start: u8;
    static _binary_obj_user_nop_end: u8;
    static _binary_obj_user_nop_size: usize;
}

const LOG2ENV: u32 = 10;
const NENV: u32 = 1 << LOG2ENV;

#[derive(Debug)]
struct EnvId(u32);

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum EnvType {
    User,
}

#[derive(Debug)]
#[allow(dead_code)]
enum EnvStatus {
    Free,
    Dying,
    Runnable,
    Running,
    NotRunnable,
}

#[repr(C)]
struct Env {
    env_tf: Trapframe,     // Saved registers
    env_id: EnvId,         // Unique environment identifier
    env_parent_id: EnvId,  // env_id of this env's parent
    env_type: EnvType,     // Indicates special system environments
    env_status: EnvStatus, // Status of the environment
    env_runs: u32,         // Number of times environment has run
    // FIXME: what type is better for env_pgdir?
    env_pgdir: Box<PageDirectory>, // Kernel virtual address of page dir
}

struct EnvTable {
    envs: [Option<Env>; NENV as usize],
}

static mut ENV_TABLE: EnvTable = EnvTable {
    envs: [None; NENV as usize],
};

static mut NEXT_ENV_ID: u32 = 1;

fn generate_env_id() -> EnvId {
    unsafe {
        let env_id = EnvId(NEXT_ENV_ID);
        NEXT_ENV_ID += 1;
        env_id
    }
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

/// Allocates and initializes a new environment.
/// On success, the new environment is stored in *newenv_store.
///
/// Returns 0 on success, < 0 on failure.  Errors include:
///	-E_NO_FREE_ENV if all NENV environments are allocated
///	-E_NO_MEM on memory exhaustion
fn env_alloc(parent_id: EnvId, typ: EnvType) -> &'static mut Env {
    unsafe {
        let mut idx = -1;
        for (i, env_opt) in ENV_TABLE.envs.iter().enumerate() {
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
        let new_id = generate_env_id();

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

        let env_opt = &mut ENV_TABLE.envs[idx as usize];
        *env_opt = Some(new_env);

        env_opt.as_mut().unwrap()
    }
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
unsafe fn load_icode(env: &mut Env, binary: *const u8) {
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
    env.env_tf.set_entry_point(elf.entry_point());
}

/// Allocates a new env with env_alloc, loads the named elf
/// binary into it with load_icode, and sets its env_type.
/// This function is ONLY called during kernel initialization,
/// before running the first user-mode environment.
/// The new env's parent ID is set to 0.
pub(crate) fn env_create(typ: EnvType) {
    let env = env_alloc(EnvId(0), typ);

    unsafe {
        let user_nop_start = &_binary_obj_user_nop_start as *const u8;
        let user_nop_end = &_binary_obj_user_nop_end as *const u8;
        let user_nop_size = &_binary_obj_user_nop_size as *const usize;

        println!("_binary_obj_user_nop_start: {:?}", user_nop_start);
        println!("_binary_obj_user_nop_end: {:?}", user_nop_end);
        println!("_binary_obj_user_nop_size: {:?}", user_nop_size);

        load_icode(env, user_nop_start);
    }
}
