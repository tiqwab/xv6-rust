// This file is base on spin crate (MIT license). See COPYRIGHT for copyright information.
// spin-rs (https://github.com/mvdnes/spin-rs)

use core::cell::UnsafeCell;
use core::fmt::Formatter;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use core::sync::atomic::{spin_loop_hint as cpu_relax, AtomicUsize, Ordering};
use core::{fmt, mem};

/// A reader-writer lock
///
/// This type of lock allows a number of readers or at most one writer at any
/// point in time. The write portion of this lock typically allows modification
/// of the underlying data (exclusive access) and the read portion of this lock
/// typically allows for read-only access (shared access).
///
/// The type parameter `T` represents the data that this lock protects. It is
/// required that `T` satisfies `Send` to be shared across tasks and `Sync` to
/// allow concurrent access through readers. The RAII guards returned from the
/// locking methods implement `Deref` (and `DerefMut` for the `write` methods)
/// to allow access to the contained of the lock.
///
/// An [`RwLockUpgradeableGuard`](RwLockUpgradeableGuard) can be upgraded to a
/// writable guard through the [`RwLockUpgradeableGuard::upgrade`](RwLockUpgradeableGuard::upgrade)
/// [`RwLockUpgradeableGuard::try_upgrade`](RwLockUpgradeableGuard::try_upgrade) functions.
/// Writable or upgradeable guards can be downgraded through their respective `downgrade`
/// functions.
///
/// Based on Facebook's
/// [`folly/RWSpinLock.h`](https://github.com/facebook/folly/blob/a0394d84f2d5c3e50ebfd0566f9d3acb52cfab5a/folly/synchronization/RWSpinLock.h).
/// This implementation is unfair to writers - if the lock always has readers, then no writers will
/// ever get a chance. Using an upgradeable lock guard can *somewhat* alleviate this issue as no
/// new readers are allowed when an upgradeable guard is held, but upgradeable guards can be taken
/// when there are existing readers. However if the lock is that highly contended and writes are
/// crucial then this implementation may be a poor choice.
///
/// # Examples
///
/// ```
/// use spin;
///
/// let lock = spin::RwLock::new(5);
///
/// // many reader locks can be held at once
/// {
///     let r1 = lock.read();
///     let r2 = lock.read();
///     assert_eq!(*r1, 5);
///     assert_eq!(*r2, 5);
/// } // read locks are dropped at this point
///
/// // only one write lock may be held, however
/// {
///     let mut w = lock.write();
///     *w += 1;
///     assert_eq!(*w, 6);
/// } // write lock is dropped here
/// ```
pub(crate) struct RwLock<T: ?Sized> {
    lock: AtomicUsize,
    data: UnsafeCell<T>,
}

const READER: usize = 1 << 2;
const UPGRADED: usize = 1 << 1;
const WRITER: usize = 1;

/// A guard from which the protected data can be read
///
/// When the guard falls out of scope it will decrement the read count,
/// potentially releasing the lock.
#[derive(Debug)]
pub(crate) struct RwLockReadGuard<'a, T: 'a + ?Sized> {
    lock: &'a AtomicUsize,
    data: NonNull<T>,
}

/// A guard to which the protected data can be written
///
/// When the guard falls out of scope it will release the lock.
#[derive(Debug)]
pub(crate) struct RwLockWriteGuard<'a, T: 'a + ?Sized> {
    lock: &'a AtomicUsize,
    data: NonNull<T>,
    #[doc(hidden)]
    _invariant: PhantomData<&'a mut T>, // why it is necessary? -> maybe for unused lifetime parameters
}

/// A guard from which the protected data can be read, and can be upgraded
/// to a writable guard if needed
///
/// No writers or other upgradeable guards can exist while this is in scope. New reader
/// creation is prevented (to alleviate writer starvation) but there may be existing readers
/// when the lock is acquired.
///
/// When the guard falls out of scope it will release the lock.
#[derive(Debug)]
pub(crate) struct RwLockUpgradeableGuard<'a, T: 'a + ?Sized> {
    lock: &'a AtomicUsize,
    data: NonNull<T>,
    #[doc(hidden)]
    _invariant: PhantomData<&'a mut T>,
}

// Same unsafe impls as `std::sync::RwLock`
unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLock<T> {}

impl<T> RwLock<T> {
    /// Creates a new spinlock wrapping the supplied data.
    ///
    /// May be used statically:
    ///
    /// ```
    /// use spin;
    ///
    /// static RW_LOCK: spin::RwLock<()> = spin::RwLock::new(());
    ///
    /// fn demo() {
    ///     let lock = RW_LOCK.read();
    ///     // do something with lock
    ///     drop(lock);
    /// }
    /// ```
    #[inline]
    pub(crate) const fn new(user_data: T) -> RwLock<T> {
        RwLock {
            lock: AtomicUsize::new(0),
            data: UnsafeCell::new(user_data),
        }
    }

    /// Consume this `RwLock`, returning the underlying data.
    #[inline]
    pub(crate) fn into_inner(self) -> T {
        // We know statically that there are no outstanding references to `self`,
        // so there's no need to lock.
        let RwLock { data, .. } = self;
        data.into_inner()
    }
}

impl<T: ?Sized> RwLock<T> {
    /// Locks this rwlock with shared read access, blocking the current thread
    /// until it can be acquired.
    ///
    /// The calling thread will be blocked until there are no more writers which
    /// hold the lock. There may be other readers currently inside the lock when
    /// this method returns. This method does not provide any guarantees with
    /// respect to the ordering of whether contentious readers or writers will
    /// acquire the lock first.
    ///
    /// Returns an RAII guard which will release this thread's shared access
    /// once it is dropped.
    ///
    /// ```
    /// let mylock = spin::RwLock::new(0);
    /// {
    ///     let mut data = mylock.read();
    ///     // The lock is now locked and the data can be read
    ///     println!("{}", *data);
    ///     // The lock is dropped
    /// }
    /// ```
    #[inline]
    pub(crate) fn read(&self) -> RwLockReadGuard<T> {
        loop {
            match self.try_read() {
                Some(guard) => return guard,
                None => cpu_relax(),
            }
        }
    }

    /// Attempt to acquire this lock with shared read access.
    ///
    /// This function will never block and will return immediately if `read`
    /// would otherwise succeed. Returns `Some` of an RAII guard which will
    /// release the shared access of this thread when dropped, or `None` if the
    /// access could not be granted. This method does not provide any
    /// guarantees with respect to the ordering of whether contentious readers
    /// or writers will acquire the lock first.
    ///
    /// ```
    /// let mylock = spin::RwLock::new(0);
    /// {
    ///     match mylock.try_read() {
    ///         Some(data) => {
    ///             // The lock is now locked and the data can be read
    ///             println!("{}", *data);
    ///             // The lock is dropped
    ///         },
    ///         None => (), // no cigar
    ///     };
    /// }
    /// ```
    #[inline]
    pub(crate) fn try_read(&self) -> Option<RwLockReadGuard<T>> {
        let value = self.lock.fetch_add(READER, Ordering::Acquire);

        // We check the UPGRADED bit here so that new readers are prevented when
        // an UPGRADED lock is held. This helps reduce writer starvation.
        if value & (WRITER | UPGRADED) != 0 {
            // Lock is taken, undo.
            self.lock.fetch_sub(READER, Ordering::Release);
            None
        } else {
            Some(RwLockReadGuard {
                lock: &self.lock,
                data: unsafe { NonNull::new_unchecked(self.data.get()) },
            })
        }
    }

    /// Force decrement the reader count.
    ///
    /// This is *extremely* unsafe if there are outstanding `RwLockReadGuard`s
    /// live, or if called more times than `read` has been called, but can be
    /// useful in FFI contexts where the caller doesn't know how to deal with
    /// RAII. The underlying atomic operation uses `Ordering::Release`.
    #[inline]
    pub(crate) unsafe fn force_read_decrement(&self) {
        debug_assert!(self.lock.load(Ordering::Relaxed) & !WRITER > 0);
        self.lock.fetch_sub(READER, Ordering::Release);
    }

    /// Force unlock exclusive write access.
    ///
    /// This is *extremely* unsafe if there are outstanding `RwLockWriteGuard`s
    /// live, or if called when there are current readers, but can be useful in
    /// FFI contexts where the caller doesn't know how to deal with RAII. The
    /// underlying atomic operation uses `Ordering::Release`.
    #[inline]
    pub(crate) unsafe fn force_write_unlock(&self) {
        debug_assert_eq!(self.lock.load(Ordering::Relaxed) & !(WRITER | UPGRADED), 0);
        self.lock.fetch_and(!(WRITER | UPGRADED), Ordering::Release);
    }

    #[inline(always)]
    fn try_write_internal(&self, strong: bool) -> Option<RwLockWriteGuard<T>> {
        if compare_exchange(
            &self.lock,
            0,
            WRITER,
            Ordering::Acquire,
            Ordering::Relaxed,
            strong,
        )
        .is_ok()
        {
            Some(RwLockWriteGuard {
                lock: &self.lock,
                data: unsafe { NonNull::new_unchecked(self.data.get()) },
                _invariant: PhantomData,
            })
        } else {
            None
        }
    }

    /// Lock this rwlock with exclusive write access, blocking the current
    /// thread until it can be acquired.
    ///
    /// This function will not return while other writers or other readers
    /// currently have access to the lock.
    ///
    /// Returns an RAII guard which will drop the write access of this rwlock
    /// when dropped.
    ///
    /// ```
    /// let mylock = spin::RwLock::new(0);
    /// {
    ///     let mut data = mylock.write();
    ///     // The lock is now locked and the data can be written
    ///     *data += 1;
    ///     // The lock is dropped
    /// }
    /// ```
    #[inline]
    pub(crate) fn write(&self) -> RwLockWriteGuard<T> {
        loop {
            match self.try_write_internal(false) {
                Some(guard) => return guard,
                None => cpu_relax(),
            }
        }
    }

    /// Attempt to lock this rwlock with exclusive write access.
    ///
    /// This function does not ever block, and it will return `None` if a call
    /// to `write` would otherwise block. If successful, an RAII guard is
    /// returned.
    ///
    /// ```
    /// let mylock = spin::RwLock::new(0);
    /// {
    ///     match mylock.try_write() {
    ///         Some(mut data) => {
    ///             // The lock is now locked and the data can be written
    ///             *data += 1;
    ///             // The lock is implicitly dropped
    ///         },
    ///         None => (), // no cigar
    ///     };
    /// }
    /// ```
    #[inline]
    pub(crate) fn try_write(&self) -> Option<RwLockWriteGuard<T>> {
        self.try_write_internal(true)
    }

    /// Obtain a readable lock guard that can later be upgraded to a writable lock guard.
    /// Upgrades can be done through the [`RwLockUpgradeableGuard::upgrade`](RwLockUpgradeableGuard::upgrade) method.
    #[inline]
    pub(crate) fn upgradeable_read(&self) -> RwLockUpgradeableGuard<T> {
        loop {
            match self.try_upgradeable_read() {
                Some(guard) => return guard,
                None => cpu_relax(),
            }
        }
    }

    /// Tries to obtain an upgradeable lock guard.
    #[inline]
    pub(crate) fn try_upgradeable_read(&self) -> Option<RwLockUpgradeableGuard<T>> {
        if self.lock.fetch_or(UPGRADED, Ordering::Acquire) & (WRITER | UPGRADED) == 0 {
            Some(RwLockUpgradeableGuard {
                lock: &self.lock,
                data: unsafe { NonNull::new_unchecked(self.data.get()) },
                _invariant: PhantomData,
            })
        } else {
            // We can't unflip the UPGRADED bit back just yet as there is another upgradeable or write lock.
            // When they unlock, they will clear the bit.
            None
        }
    }

    pub(crate) fn get_mut(&mut self) -> &mut T {
        // We know statically that there are no other references to `self`, so
        // there's no need to lock the inner lock.
        unsafe { &mut *self.data.get() }
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for RwLock<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.try_read() {
            Some(guard) => write!(f, "RwLock {{ data: ")
                .and_then(|()| (&*guard).fmt(f))
                .and_then(|()| write!(f, "}}")),
            None => write!(f, "RwLock {{ <locked> }}"),
        }
    }
}

impl<T: ?Sized + Default> Default for RwLock<T> {
    fn default() -> RwLock<T> {
        RwLock::new(Default::default())
    }
}

impl<'rwlock, T: ?Sized> RwLockUpgradeableGuard<'rwlock, T> {
    #[inline(always)]
    fn try_upgrade_internal(self, strong: bool) -> Result<RwLockWriteGuard<'rwlock, T>, Self> {
        if compare_exchange(
            &self.lock,
            UPGRADED,
            WRITER,
            Ordering::Acquire,
            Ordering::Relaxed,
            strong,
        )
        .is_ok()
        {
            // upgrade successful
            let out = Ok(RwLockWriteGuard {
                lock: &self.lock,
                data: self.data,
                _invariant: PhantomData,
            });

            // forget the old guard so its destructor doesn't run
            mem::forget(self);

            out
        } else {
            Err(self)
        }
    }

    /// Upgrades an upgradeable lock guard to a writable lock guard.
    ///
    /// ```
    /// let mylock = spin::RwLock::new(0);
    ///
    /// let upgradeable = mylock.upgradeable_read(); // Readable, but not yet writable
    /// let writable = upgradeable.upgrade();
    /// ```
    #[inline]
    pub(crate) fn upgrade(mut self) -> RwLockWriteGuard<'rwlock, T> {
        loop {
            self = match self.try_upgrade_internal(false) {
                Ok(guard) => return guard,
                Err(e) => e,
            };

            cpu_relax();
        }
    }

    /// Tries to upgrade an upgradeable lock guard to a writable lock guard.
    ///
    /// ```
    /// let mylock = spin::RwLock::new(0);
    /// let upgradeable = mylock.upgradeable_read(); // Readable, but not yet writable
    ///
    /// match upgradeable.try_upgrade() {
    ///     Ok(writable) => /* upgrade successful - use writable lock guard */ (),
    ///     Err(upgradeable) => /* upgrade unsuccessful */ (),
    /// };
    /// ```
    #[inline]
    pub(crate) fn try_upgrade(self) -> Result<RwLockWriteGuard<'rwlock, T>, Self> {
        self.try_upgrade_internal(true)
    }

    /// Downgrades the upgradeable lock guard to a readable, shared lock guard. Cannot fail and is guaranteed not to spin.
    ///
    /// ```
    /// let mylock = spin::RwLock::new(1);
    ///
    /// let upgradeable = mylock.upgradeable_read();
    /// assert!(mylock.try_read().is_none());
    /// assert_eq!(*upgradeable, 1);
    ///
    /// let readable = upgradeable.downgrade(); // This is guaranteed not to spin
    /// assert!(mylock.try_read().is_some());
    /// assert_eq!(*readable, 1);
    /// ```
    #[inline]
    pub(crate) fn downgrade(self) -> RwLockReadGuard<'rwlock, T> {
        // reserve the read guard for ourselves
        self.lock.fetch_add(READER, Ordering::Acquire);

        RwLockReadGuard {
            lock: &self.lock,
            data: self.data,
        }

        // dropping self removes the UPGRADED bit
    }
}

impl<'rwlock, T: ?Sized> RwLockWriteGuard<'rwlock, T> {
    /// Downgrades the writable lock guard to a readable, shared lock guard. Cannot fail and is guaranteed not to spin.
    ///
    /// ```
    /// let mylock = spin::RwLock::new(0);
    ///
    /// let mut writable = mylock.write();
    /// *writable = 1;
    ///
    /// let readable = writable.downgrade(); // This is guaranteed not to spin
    /// # let readable_2 = mylock.try_read().unwrap();
    /// assert_eq!(*readable, 1);
    /// ```
    #[inline]
    pub(crate) fn downgrade(self) -> RwLockReadGuard<'rwlock, T> {
        // reserve the read guard for ourselves
        self.lock.fetch_add(READER, Ordering::Acquire);

        RwLockReadGuard {
            lock: &self.lock,
            data: self.data,
        }

        // dropping self removes the WRITER bit
    }
}

impl<'rwlock, T: ?Sized> Deref for RwLockReadGuard<'rwlock, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { self.data.as_ref() }
    }
}

impl<'rwlock, T: ?Sized> Deref for RwLockUpgradeableGuard<'rwlock, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.data.as_ref() }
    }
}

impl<'rwlock, T: ?Sized> Deref for RwLockWriteGuard<'rwlock, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.data.as_ref() }
    }
}

impl<'rwlock, T: ?Sized> DerefMut for RwLockWriteGuard<'rwlock, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { self.data.as_mut() }
    }
}

impl<'rwlock, T: ?Sized> Drop for RwLockReadGuard<'rwlock, T> {
    fn drop(&mut self) {
        debug_assert!(self.lock.load(Ordering::Relaxed) & !(WRITER | UPGRADED) > 0);
        self.lock.fetch_sub(READER, Ordering::Release);
    }
}

impl<'rwlock, T: ?Sized> Drop for RwLockUpgradeableGuard<'rwlock, T> {
    fn drop(&mut self) {
        debug_assert_eq!(
            self.lock.load(Ordering::Relaxed) & (WRITER | UPGRADED),
            UPGRADED
        );
        self.lock.fetch_sub(UPGRADED, Ordering::AcqRel);
    }
}

impl<'rwlock, T: ?Sized> Drop for RwLockWriteGuard<'rwlock, T> {
    fn drop(&mut self) {
        debug_assert_eq!(self.lock.load(Ordering::Relaxed) & WRITER, WRITER);

        // Writer is responsible for clearing both WRITER and UPGRADED bits.
        // The UPGRADED bit may be set if an upgradeable lock attempts an upgrade while this lock is held.
        self.lock.fetch_and(!(WRITER | UPGRADED), Ordering::Release);
    }
}

#[inline(always)]
fn compare_exchange(
    atomic: &AtomicUsize,
    current: usize,
    new: usize,
    success: Ordering,
    failure: Ordering,
    strong: bool,
) -> Result<usize, usize> {
    if strong {
        atomic.compare_exchange(current, new, success, failure)
    } else {
        atomic.compare_exchange_weak(current, new, success, failure)
    }
}
