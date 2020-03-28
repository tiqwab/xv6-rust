// This file is base on spin crate (MIT license). See COPYRIGHT for copyright information.
// spin-rs (https://github.com/mvdnes/spin-rs)

use core::cell::UnsafeCell;
use core::fmt;
use core::fmt::Formatter;
use core::hint::unreachable_unchecked as unreachable;
use core::sync::atomic::{spin_loop_hint as cpu_relax, AtomicUsize, Ordering};

/// A synchronization primitive which can be used to run a one-time global
/// initialization. Unlike its std equivalent, this is generalized so that the
/// closure returns a value and it is stored. Once therefore acts something like
/// a future, too.
///
/// # Examples
///
/// ```
/// use spin;
///
/// static START: spin::Once<()> = spin::Once::new();
///
/// START.call_once(|| {
///     // run initialization here
/// });
/// ```
pub(crate) struct Once<T> {
    state: AtomicUsize,
    data: UnsafeCell<Option<T>>,
}

impl<T: fmt::Debug> fmt::Debug for Once<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.try_get() {
            Some(s) => write!(f, "Once {{ data: ")
                .and_then(|()| s.fmt(f))
                .and_then(|()| write!(f, "}}")),
            None => write!(f, "Once {{ <uninitialized> }}"),
        }
    }
}

// Same unsafe impls as `std::sync::RwLock`, because this also allows for
// concurrent reads.
unsafe impl<T: Send + Sync> Sync for Once<T> {}
unsafe impl<T: Send> Send for Once<T> {}

// Four states that a Once can be in, encoded into the lower bits of `state` in
// the Once structure
const INCOMPLETE: usize = 0x0;
const RUNNING: usize = 0x01;
const COMPLETE: usize = 0x2;
const PANICKED: usize = 0x3;

impl<T> Once<T> {
    /// Initialization constant of `Once`.
    pub(crate) const INIT: Self = Once {
        state: AtomicUsize::new(INCOMPLETE),
        data: UnsafeCell::new(None),
    };

    /// Create a new `Once` value.
    pub(crate) const fn new() -> Once<T> {
        Self::INIT
    }

    fn force_get(&self) -> &T {
        match unsafe { &*self.data.get() }.as_ref() {
            None => unsafe { unreachable() },
            Some(p) => p,
        }
    }

    /// Performs an initialization routine once and only once. The given closure
    /// will be executed if this is the first time `call_once` has been called,
    /// and otherwise the routine will *not* be invoked.
    ///
    /// This method will block the calling thread if another initialization
    /// routine is currently running.
    ///
    /// When this function returns, it is guaranteed that some initialization
    /// has run and completed (it may not be the closure specified). The
    /// returned pointer will point to the result from the closure that was
    /// run.
    ///
    /// # Examples
    ///
    /// ```
    /// use spin;
    ///
    /// static INIT: spin::Once<usize> = spin::Once::new();
    ///
    /// fn get_cached_val() -> usize {
    ///     *INIT.call_once(expensive_computation)
    /// }
    ///
    /// fn expensive_computation() -> usize {
    ///     // ...
    /// # 2
    /// }
    /// ```
    pub fn call_once<F: FnOnce() -> T>(&self, builder: F) -> &T {
        let mut status = self.state.load(Ordering::SeqCst);

        if status == INCOMPLETE {
            // We init
            status = self
                .state
                .compare_and_swap(INCOMPLETE, RUNNING, Ordering::SeqCst);

            // We use a guard (Finish) to catch panics caused by builder
            // The state changes into PANICKED in Drop of Finish.
            let mut finish = Finish {
                state: &self.state,
                panicked: true,
            };
            unsafe { *self.data.get() = Some(builder()) };
            finish.panicked = false;

            status = COMPLETE;
            self.state.store(status, Ordering::SeqCst);

            // This next line is strictly an optimization
            return self.force_get();
        }

        loop {
            match status {
                INCOMPLETE => unreachable!(),
                RUNNING => {
                    // We spin
                    cpu_relax();
                    status = self.state.load(Ordering::SeqCst)
                }
                PANICKED => panic!("Once has panicked"),
                COMPLETE => return self.force_get(),
                _ => unreachable!(),
            }
        }
    }

    /// Return a pointer iff the `Once` was previously initialized
    pub(crate) fn try_get(&self) -> Option<&T> {
        match self.state.load(Ordering::SeqCst) {
            COMPLETE => Some(self.force_get()),
            _ => None,
        }
    }

    /// Like try_get, but will spin if the `Once` is in the process of being initialized
    pub(crate) fn wait(&self) -> Option<&T> {
        loop {
            match self.state.load(Ordering::SeqCst) {
                INCOMPLETE => return None,
                RUNNING => cpu_relax(),
                COMPLETE => return Some(self.force_get()),
                PANICKED => panic!("Once has panicked"),
                _ => unreachable!(),
            }
        }
    }
}

struct Finish<'a> {
    state: &'a AtomicUsize,
    panicked: bool,
}

impl<'a> Drop for Finish<'a> {
    fn drop(&mut self) {
        if self.panicked {
            self.state.store(PANICKED, Ordering::SeqCst);
        }
    }
}
