use std::cell::UnsafeCell;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};

use crossbeam::utils::CachePadded;

use crate::global_yield_now;

// region guards

pub struct ReadLockGuard<'rw_lock, T> {
    local_rw_lock: &'rw_lock RWLock<T>,
}

impl<'rw_lock, T> ReadLockGuard<'rw_lock, T> {
    #[inline(always)]
    fn new(local_rw_lock: &'rw_lock RWLock<T>) -> Self {
        Self { local_rw_lock }
    }

    #[inline(always)]
    /// Unlocks the mutex. Calling `guard.unlock()` is equivalent to calling `drop(guard)`.
    /// This was done to improve readability.
    ///
    /// # Attention
    ///
    /// Even if you doesn't call `guard.unlock()`,
    /// the mutex will be unlocked after the `guard` is dropped.
    pub fn unlock(self) {}

    #[inline(always)]
    pub unsafe fn leak(self) -> &'rw_lock T {
        let static_mutex = unsafe { mem::transmute(self.local_rw_lock) };
        mem::forget(self);

        static_mutex
    }
}

impl<'rw_lock, T> Deref for ReadLockGuard<'rw_lock, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.local_rw_lock.value.get() }
    }
}

impl<'rw_lock, T> Drop for ReadLockGuard<'rw_lock, T> {
    fn drop(&mut self) {
        unsafe {
            self.local_rw_lock.unlock_read();
        }
    }
}

pub struct WriteLockGuard<'rw_lock, T> {
    local_rw_lock: &'rw_lock RWLock<T>,
}

impl<'rw_lock, T> WriteLockGuard<'rw_lock, T> {
    #[inline(always)]
    fn new(local_rw_lock: &'rw_lock RWLock<T>) -> Self {
        Self { local_rw_lock }
    }

    #[inline(always)]
    /// Unlocks the mutex. Calling `guard.unlock()` is equivalent to calling `drop(guard)`.
    /// This was done to improve readability.
    ///
    /// # Attention
    ///
    /// Even if you doesn't call `guard.unlock()`,
    /// the mutex will be unlocked after the `guard` is dropped.
    pub fn unlock(self) {}

    #[inline(always)]
    pub unsafe fn leak(self) -> &'rw_lock T {
        let static_mutex = unsafe { mem::transmute(self.local_rw_lock) };
        mem::forget(self);

        static_mutex
    }
}

impl<'rw_lock, T> Deref for WriteLockGuard<'rw_lock, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.local_rw_lock.value.get() }
    }
}

impl<'rw_lock, T> DerefMut for WriteLockGuard<'rw_lock, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.local_rw_lock.value.get() }
    }
}

impl<'rw_lock, T> Drop for WriteLockGuard<'rw_lock, T> {
    fn drop(&mut self) {
        unsafe {
            self.local_rw_lock.unlock_write();
        }
    }
}

// endregion

pub struct RWLock<T> {
    number_of_readers: CachePadded<AtomicIsize>,
    value: UnsafeCell<T>,
}

impl<T> RWLock<T> {
    #[inline(always)]
    pub fn new(value: T) -> RWLock<T> {
        RWLock {
            number_of_readers: CachePadded::new(AtomicIsize::new(0)),
            value: UnsafeCell::new(value),
        }
    }

    #[inline(always)]
    pub async fn write(&self) -> WriteLockGuard<T> {
        loop {
            match self.try_write() {
                Some(guard) => return guard,
                None => global_yield_now().await,
            }
        }
    }

    #[inline(always)]
    pub fn try_write(&self) -> Option<WriteLockGuard<T>> {
        let was_swapped = self.number_of_readers
            .compare_exchange(0, -1, Acquire, Relaxed)
            .is_ok();
        if !was_swapped {
            None
        } else {
            Some(WriteLockGuard::new(self))
        }
    }

    #[inline(always)]
    pub async fn read(&self) -> ReadLockGuard<T> {
        loop {
            match self.try_read() {
                Some(guard) => return guard,
                None => global_yield_now().await,
            }
        }
    }

    #[inline(always)]
    pub fn try_read(&self) -> Option<ReadLockGuard<T>> {
        loop {
            let number_of_readers = self.number_of_readers.load(Acquire);
            if number_of_readers >= 0 {
                if self
                    .number_of_readers
                    .compare_exchange(number_of_readers, number_of_readers + 1, Acquire, Relaxed)
                    .is_ok()
                {
                    break Some(ReadLockGuard::new(self));
                }
            } else {
                break None;
            }
        }
    }

    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    #[inline(always)]
    pub unsafe fn unlock_read(&self) {
        if cfg!(debug_assertions) {
            let current = self.number_of_readers.load(Acquire);
            if current < 0 {
                panic!("NaiveRWLock is locked for write");
            }

            if current == 0 {
                panic!("NaiveRWLock is already unlocked");
            }
        }

        self.number_of_readers.fetch_sub(1, Release);
    }

    #[inline(always)]
    pub unsafe fn unlock_write(&self) {
        if cfg!(debug_assertions) {
            let current = self.number_of_readers.load(Acquire);
            if current == 0 {
                panic!("NaiveRWLock is already unlocked");
            }

            if current > 0 {
                panic!("NaiveRWLock is locked for read");
            }
        }

        self.number_of_readers.store(0, Release);
    }

    #[inline(always)]
    pub unsafe fn get_read_locked(&self) -> &T {
        if cfg!(debug_assertions) {
            let current = self.number_of_readers.load(Acquire);
            if current < 0 {
                panic!("NaiveRWLock is locked for write");
            }

            if current == 0 {
                panic!("NaiveRWLock is unlocked but get_read_locked is called");
            }
        }

        unsafe { &*self.value.get() }
    }

    #[inline(always)]
    pub unsafe fn get_write_locked(&self) -> &mut T {
        if cfg!(debug_assertions) {
            let current = self.number_of_readers.load(Acquire);
            if current == 0 {
                panic!("NaiveRWLock is unlocked but get_write_locked is called");
            }

            if current > 0 {
                panic!("NaiveRWLock is locked for read");
            }
        }

        unsafe { &mut *self.value.get() }
    }
}

unsafe impl<T: Send + Sync> Sync for RWLock<T> {}
unsafe impl<T: Send> Send for RWLock<T> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[orengine_macros::test_global]
    fn test_rw_lock() {
        let rw_lock = RWLock::new(0);

        let guard = rw_lock.read().await;
        assert!(rw_lock.try_read().is_some());
        assert!(rw_lock.try_write().is_none());
        guard.unlock();

        let mut guard = rw_lock.write().await;
        assert!(rw_lock.try_read().is_none());
        assert!(rw_lock.try_write().is_none());
        *guard += 1;
        assert_eq!(*guard, 1);
        guard.unlock();

        assert_eq!(*rw_lock.try_read().expect("failed to get read lock"), 1);
        assert_eq!(*rw_lock.try_read().expect("failed to get read lock"), 1);
    }
}
