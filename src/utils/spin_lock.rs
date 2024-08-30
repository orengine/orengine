use std::cell::UnsafeCell;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use crossbeam::utils::{Backoff, CachePadded};

pub struct SpinLockGuard<'spin_lock, T> {
    spin_lock: &'spin_lock SpinLock<T>
}

impl<'spin_lock, T> SpinLockGuard<'spin_lock, T> {
    #[inline(always)]
    pub(crate) fn new(spin_lock: &'spin_lock SpinLock<T>) -> Self {
        Self { spin_lock }
    }

    #[inline(always)]
    pub fn spin_lock(&self) -> &SpinLock<T> {
        &self.spin_lock
    }

    #[inline(always)]
    /// Unlocks the spin_lock. Calling `guard.unlock()` is equivalent to calling `drop(guard)`.
    /// This was done to improve readability.
    ///
    /// # Attention
    ///
    /// Even if you doesn't call `guard.unlock()`,
    /// the spin_lock will be unlocked after the `guard` is dropped.
    pub fn unlock(self) {}

    pub unsafe fn leak(self) -> *const CachePadded<AtomicBool> {
        let atomic_ptr = &self.spin_lock.is_locked;
        mem::forget(self);

        atomic_ptr
    }
}

impl<'spin_lock, T> Deref for SpinLockGuard<'spin_lock, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.spin_lock.value.get() }
    }
}

impl<'spin_lock, T> DerefMut for SpinLockGuard<'spin_lock, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.spin_lock.value.get() }
    }
}

impl<'spin_lock, T> Drop for SpinLockGuard<'spin_lock, T> {
    fn drop(&mut self) {
        unsafe { self.spin_lock.unlock() };
    }
}

pub struct SpinLock<T> {
    is_locked: CachePadded<AtomicBool>,
    value: UnsafeCell<T>,
}

impl<T> SpinLock<T> {
    #[inline(always)]
    pub fn new(value: T) -> SpinLock<T> {
        SpinLock {
            is_locked: CachePadded::new(AtomicBool::new(false)),
            value: UnsafeCell::new(value),
        }
    }

    #[inline(always)]
    pub fn lock(&self) -> SpinLockGuard<T> {
        let backoff = Backoff::new();
        loop {
            if let Some(guard) = self.try_lock() {
                return guard;
            }
            backoff.spin();
        }
    }

    #[inline(always)]
    pub fn try_lock(&self) -> Option<SpinLockGuard<T>> {
        if self.is_locked.compare_exchange(false, true, Acquire, Relaxed).is_ok() {
            Some(SpinLockGuard::new(self))
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    #[inline(always)]
    pub unsafe fn get_locked(&self) -> &mut T {
        debug_assert!(self.is_locked.load(Acquire));
        &mut *self.value.get()
    }

    #[inline(always)]
    pub unsafe fn unlock(&self) {
        self.is_locked.store(false, Release);
    }
}

unsafe impl<T: Send> Sync for SpinLock<T> {}
unsafe impl<T: Send> Send for SpinLock<T> {}