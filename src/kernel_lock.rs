use crate::spinlock::{Mutex, MutexGuard};

static KERNEL_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn lock_kernel<'a>() -> MutexGuard<'a, ()> {
    KERNEL_LOCK.lock()
}
