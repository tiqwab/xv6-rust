use crate::env;
use crate::env::EnvTable;
use crate::mpconfig;
use crate::spinlock::MutexGuard;

/// Choose a user environment to run and run it.
pub(crate) fn sched_yield() {
    // Implement simple round-robin scheduling.
    //
    // Search through 'envs' for an ENV_RUNNABLE environment in
    // circular fashion starting just after the env this CPU was
    // last running.  Switch to the first such environment found.
    //
    // If no envs are runnable, but the environment previously
    // running on this CPU is still ENV_RUNNING, it's okay to
    // choose that environment.
    //
    // Never choose an environment that's currently running on
    // another CPU (env_status == ENV_RUNNING). If there are
    // no runnable environments, simply drop through to the code
    // below to halt the cpu.

    let mut env_table = env::env_table();
    let env_id_opt = env_table.find_runnable();
    match env_id_opt {
        Some(env_id) => {
            env::env_run(env_id, env_table);
        }
        None => {
            sched_halt(env_table);
        }
    }
}

pub(crate) fn sched_halt(table: MutexGuard<EnvTable>) {
    println!("sched_halt: there is no runnable envs.");
    drop(table);
    loop {}
}
