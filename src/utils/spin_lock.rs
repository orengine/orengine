//! This module provides an __blocking__ mutex (e.g. [`std::sync::Mutex`]) type [`SpinLock`].
//!
//! It allows for __blocking__ locking and unlocking, and provides
//! ownership-based locking through [`SpinLockGuard`].
//!
//! It locks the current __thread__ until it acquires the lock. Use it only for short locks and
//! only if it is not possible to use asynchronous locking.
use crossbeam::utils::{Backoff, CachePadded};
use std::cell::UnsafeCell;
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::sync::atomic;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};

/// An RAII implementation of a "scoped lock" of a mutex. When this structure is
/// dropped (falls out of scope), the lock will be unlocked.
///
/// The data protected by the mutex can be accessed through this guard via its
/// [`Deref`] and [`DerefMut`] implementations.
///
/// This structure is created by the [`lock`](SpinLock::lock)
/// and [`try_lock`](SpinLock::try_lock) methods on [`SpinLock`].
pub struct SpinLockGuard<'spin_lock, T: ?Sized> {
    spin_lock: &'spin_lock SpinLock<T>,
}

impl<'spin_lock, T: ?Sized> SpinLockGuard<'spin_lock, T> {
    /// Creates a new [`SpinLockGuard`].
    #[inline]
    pub(crate) fn new(spin_lock: &'spin_lock SpinLock<T>) -> Self {
        Self { spin_lock }
    }

    /// Returns a reference to the original [`SpinLock`].
    #[inline]
    pub fn spin_lock(&self) -> &SpinLock<T> {
        self.spin_lock
    }

    /// Unlocks the [`spin_lock`](SpinLock). Calling `guard.unlock()` is equivalent to
    /// calling `drop(guard)`. This was done to improve readability.
    ///
    /// # Attention
    ///
    /// Even if you doesn't call `guard.unlock()`,
    /// the [`spin_lock`](SpinLock) will be unlocked after the `guard` is dropped.
    #[inline]
    pub fn unlock(self) {}

    /// Returns a reference to the original [`SpinLock`].
    ///
    /// The lock will never be unlocked.
    ///
    /// # Safety
    ///
    /// The mutex is unlocked by calling [`SpinLock::unlock`] later.
    #[inline]
    pub unsafe fn leak(self) -> *const CachePadded<AtomicBool> {
        &ManuallyDrop::new(self).spin_lock.is_locked
    }

    /// Returns a reference to the [`CachePadded<AtomicBool>`]
    /// associated with the original [`SpinLock`] to
    /// [`call`](crate::Executor::invoke_call)
    /// [`ReleaseAtomicBool`](crate::runtime::call::Call::ReleaseAtomicBool).
    ///
    /// # Safety
    ///
    /// The mutex is unlocked by calling [`SpinLock::unlock`] later
    /// or by [calling](crate::Executor::invoke_call)
    /// [`ReleaseAtomicBool`](crate::runtime::call::Call::ReleaseAtomicBool).
    #[inline]
    pub unsafe fn leak_to_atomic(self) -> &'spin_lock CachePadded<AtomicBool> {
        &ManuallyDrop::new(self).spin_lock.is_locked
    }
}

impl<T: ?Sized> Deref for SpinLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.spin_lock.value.get() }
    }
}

impl<T: ?Sized> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.spin_lock.value.get() }
    }
}

impl<T: ?Sized> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        unsafe { self.spin_lock.unlock() };
    }
}

/// A mutual exclusion primitive useful for protecting shared data.
///
/// This mutex will block __thread__ waiting for the lock to become available. The
/// mutex can be created via a [`new`](SpinLock::new) constructor. Each mutex has a type parameter
/// which represents the data that it is protecting. The data can be accessed
/// through the RAII guards returned from [`lock`](SpinLock::lock) and
/// [`try_lock`](SpinLock::try_lock),
/// which guarantees that the data is only ever accessed when the mutex is locked, or
/// with an unsafe method [`get_locked`](SpinLock::get_locked).
///
/// # The difference between `SpinLock` and other mutexes
///
/// It locks the current __thread__ until it acquires the lock. Use it only for short locks and
/// only if it is not possible to use asynchronous locking.
pub struct SpinLock<T: ?Sized> {
    is_locked: CachePadded<AtomicBool>,
    value: UnsafeCell<T>,
}

impl<T: ?Sized> SpinLock<T> {
    /// Creates a new [`SpinLock`].
    pub const fn new(value: T) -> Self
    where
        T: Sized,
    {
        Self {
            is_locked: CachePadded::new(AtomicBool::new(false)),
            value: UnsafeCell::new(value),
        }
    }

    /// Blocks the current __thread__ until it acquires the lock.
    #[inline]
    pub fn lock(&self) -> SpinLockGuard<T> {
        let backoff = Backoff::new();
        loop {
            if let Some(guard) = self.try_lock() {
                atomic::fence(Acquire);

                return guard;
            }
            backoff.spin();
        }
    }

    /// If `SpinLock` is unlocked, returns [`SpinLockGuard`] that allows access to the inner value,
    /// otherwise returns [`None`].
    #[inline]
    pub fn try_lock(&self) -> Option<SpinLockGuard<T>> {
        if self
            .is_locked
            .compare_exchange_weak(false, true, Acquire, Relaxed)
            .is_ok()
        {
            Some(SpinLockGuard::new(self))
        } else {
            None
        }
    }

    /// Returns a reference to the underlying data. It is safe because it uses `&mut self`.
    #[inline]
    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    /// Unlocks the mutex.
    ///
    /// # Safety
    ///
    /// - The `SpinLock` must be locked.
    ///
    /// - No other threads has an ownership of this `lock`.
    #[inline]
    pub unsafe fn unlock(&self) {
        debug_assert!(self.is_locked.load(Acquire));
        self.is_locked.store(false, Release);
    }

    /// Returns a reference to the inner value.
    ///
    /// # Safety
    ///
    /// - The `SpinLock` must be locked.
    ///
    /// - And only current task has an ownership of this `SpinLock`.
    #[inline]
    #[allow(
        clippy::mut_from_ref,
        reason = "The caller guarantees safety using this code"
    )]
    pub unsafe fn get_locked(&self) -> &mut T {
        debug_assert!(self.is_locked.load(Acquire));
        unsafe { &mut *self.value.get() }
    }
}

unsafe impl<T: ?Sized + Send + Sync> Sync for SpinLock<T> {}
unsafe impl<T: ?Sized + Send> Send for SpinLock<T> {}
impl<T: ?Sized + UnwindSafe> UnwindSafe for SpinLock<T> {}
impl<T: ?Sized + RefUnwindSafe> RefUnwindSafe for SpinLock<T> {}

#[cfg(test)]
mod tests {
    use crate as orengine;
    use crate::sync::{AsyncWaitGroup, WaitGroup};
    use crate::test::sched_future_to_another_thread;
    use crate::utils::SpinLock;
    use std::sync::Arc;

    #[orengine::test::test_shared]
    fn test_try_mutex() {
        let mutex = Arc::new(SpinLock::new(false));
        let mutex_clone = mutex.clone();
        let lock_wg = Arc::new(WaitGroup::new());
        let lock_wg_clone = lock_wg.clone();
        let unlock_wg = Arc::new(WaitGroup::new());
        let unlock_wg_clone = unlock_wg.clone();
        let second_lock = Arc::new(WaitGroup::new());
        let second_lock_clone = second_lock.clone();

        lock_wg.add(1);
        unlock_wg.add(1);
        sched_future_to_another_thread(async move {
            let mut value = mutex_clone.lock();
            println!("1");
            lock_wg_clone.done();
            unlock_wg_clone.wait().await;
            println!("4");
            *value = true;
            drop(value);
            second_lock_clone.done();
            println!("5");
        });

        lock_wg.wait().await;
        println!("2");
        let value = mutex.try_lock();
        println!("3");
        assert!(value.is_none());
        second_lock.inc();
        unlock_wg.done();

        second_lock.wait().await;
        let value = mutex.try_lock();
        println!("6");
        match value {
            Some(v) => assert!(*v, "not waited"),
            None => panic!("can't acquire lock"),
        }
    }

    #[orengine::test::test_shared]
    fn stress_test_mutex() {
        const PAR: usize = 4;
        const TRIES: usize = 1000;

        fn work_with_lock(mutex: &SpinLock<usize>, wg: &WaitGroup) {
            let mut lock = mutex.lock();
            *lock += 1;
            lock.unlock();

            wg.done();
        }

        for _ in 0..20 {
            let mutex = Arc::new(SpinLock::new(0));
            let wg = Arc::new(WaitGroup::new());
            wg.add(PAR * TRIES);
            for _ in 1..PAR {
                let wg = wg.clone();
                let mutex = mutex.clone();
                sched_future_to_another_thread(async move {
                    for _ in 0..TRIES {
                        work_with_lock(&mutex, &wg);
                    }
                });
            }

            for _ in 0..TRIES {
                work_with_lock(&mutex, &wg);
            }

            wg.wait().await;

            assert_eq!(*mutex.lock(), TRIES * PAR);
        }
    }
}
