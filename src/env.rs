use alloc::boxed::Box;
use core::ptr::{null, null_mut};

use crate::elf::{Elf, ElfParser, Proghdr, ProghdrType, Secthdr, SecthdrType, ELF_MAGIC};
use crate::pmap::PageDirectory;
use crate::trap::Trapframe;

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
fn load_icode(env: &mut Env, binary: *const u8) {
    // TODO:
    // - [x] include binary to kernel
    // - [x] prepare struct for Elf
    // - [ ] implement load_icode

    unsafe {
        let elf = {
            let e = ElfParser::new(binary);
            e.expect("binary is not elf")
        };

        for ph in elf.program_headers() {
            println!("{:?}", ph.p_type);
        }
    }
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

    // void
    // env_create(uint8_t *binary, enum EnvType type)
    // {
    //     struct Env *e;
    //     if (env_alloc(&e, 0) < 0) {
    //         panic("failed in env_create");
    //     }
    //     e->env_type = type;
    //     load_icode(e, binary);
    // }
}
