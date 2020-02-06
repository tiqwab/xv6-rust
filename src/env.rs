use core::ptr::null_mut;

use crate::pmap::PageDirectory;
use alloc::boxed::Box;

const LOG2ENV: u32 = 10;
const NENV: u32 = 1 << LOG2ENV;

#[derive(Debug)]
struct Trapframe {}

#[derive(Debug)]
struct EnvId(u32);

#[derive(Debug)]
#[allow(dead_code)]
enum EnvType {
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

impl Env {
    fn new() -> Env {
        Env {
            env_tf: Trapframe {},
            env_id: EnvId(0),
            env_parent_id: EnvId(0),
            env_type: EnvType::User,
            env_status: EnvStatus::Free,
            env_runs: 0,
            env_pgdir: Box::new(PageDirectory::new()),
        }
    }
}

struct EnvTable {
    envs: [Option<Env>; NENV as usize],
}

static mut ENV_TABLE: EnvTable = EnvTable {
    envs: [None; NENV as usize],
};
