//! This module provides an asynchronous rw_lock (e.g. [`std::sync::RwLock`]) type [`RWLock`].
//! It allows for asynchronous read or write locking and unlocking, and provides
//! ownership-based locking through [`ReadLockGuard`] and [`WriteLockGuard`].
use std::cell::UnsafeCell;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};

use crossbeam::utils::CachePadded;

use crate::yield_now;

// region guards

/// RAII structure used to release the shared read access of a lock when
/// dropped.
///
/// This structure is created by the [`RWLock::read`](RWLock::read)
/// and [`RWLock::try_read`](RWLock::try_read).
pub struct ReadLockGuard<'rw_lock, T> {
    rw_lock: &'rw_lock RWLock<T>,
}

impl<'rw_lock, T> ReadLockGuard<'rw_lock, T> {
    /// Creates a new `ReadLockGuard`.
    #[inline(always)]
    fn new(rw_lock: &'rw_lock RWLock<T>) -> Self {
        Self { rw_lock }
    }

    /// Returns a reference to the original [`RWLock`].
    #[inline(always)]
    pub fn rw_lock(&self) -> &RWLock<T> {
        &self.rw_lock
    }

    /// Release the shared read access of a lock.
    /// Calling `guard.unlock()` is equivalent to calling `drop(guard)`.
    /// This was done to improve readability.
    ///
    /// # Attention
    ///
    /// Even if you doesn't call `guard.unlock()`,
    /// the mutex will be unlocked after the `guard` is dropped.
    #[inline(always)]
    pub fn unlock(self) {}

    /// Returns a reference to the original [`RWLock`].
    ///
    /// The read portion of this lock will not be released.
    ///
    /// # Safety
    ///
    /// The rw_lock is read unlocked by calling [`RWLock::read_unlock`] later.
    #[inline(always)]
    pub unsafe fn leak(self) -> &'rw_lock T {
        let static_mutex = unsafe { mem::transmute(self.rw_lock) };
        mem::forget(self);

        static_mutex
    }
}

impl<'rw_lock, T> Deref for ReadLockGuard<'rw_lock, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.rw_lock.value.get() }
    }
}

impl<'rw_lock, T> Drop for ReadLockGuard<'rw_lock, T> {
    fn drop(&mut self) {
        unsafe {
            self.rw_lock.read_unlock();
        }
    }
}

/// RAII structure used to release the exclusive write access of a lock when
/// dropped.
///
/// This structure is created by the [`RWLock::write`](RWLock::write)
/// and [`RWLock::try_write`](RWLock::try_write).
pub struct WriteLockGuard<'rw_lock, T> {
    rw_lock: &'rw_lock RWLock<T>,
}

impl<'rw_lock, T> WriteLockGuard<'rw_lock, T> {
    /// Creates a new `WriteLockGuard`.
    #[inline(always)]
    fn new(rw_lock: &'rw_lock RWLock<T>) -> Self {
        Self { rw_lock }
    }

    /// Returns a reference to the original [`RWLock`].
    #[inline(always)]
    pub fn rw_lock(&self) -> &RWLock<T> {
        &self.rw_lock
    }

    /// Release the exclusive write access of a lock.
    /// Calling `guard.unlock()` is equivalent to calling `drop(guard)`.
    /// This was done to improve readability.
    ///
    /// # Attention
    ///
    /// Even if you doesn't call `guard.unlock()`,
    /// the mutex will be unlocked after the `guard` is dropped.
    #[inline(always)]
    pub fn unlock(self) {}

    /// Returns a reference to the original [`RWLock`].
    ///
    /// The write portion of this lock will not be released.
    ///
    /// # Safety
    ///
    /// The rw_lock is write unlocked by calling [`RWLock::write_unlock`] later.
    #[inline(always)]
    pub unsafe fn leak(self) -> &'rw_lock T {
        let static_mutex = unsafe { mem::transmute(self.rw_lock) };
        mem::forget(self);

        static_mutex
    }
}

impl<'rw_lock, T> Deref for WriteLockGuard<'rw_lock, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.rw_lock.value.get() }
    }
}

impl<'rw_lock, T> DerefMut for WriteLockGuard<'rw_lock, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.rw_lock.value.get() }
    }
}

impl<'rw_lock, T> Drop for WriteLockGuard<'rw_lock, T> {
    fn drop(&mut self) {
        unsafe {
            self.rw_lock.write_unlock();
        }
    }
}

// endregion

/// A reader-writer lock.
///
/// This type of lock allows a number of readers or at most one writer at any
/// point in time. The write portion of this lock typically allows modification
/// of the underlying data (exclusive access) and the read portion of this lock
/// typically allows for read-only access (shared access).
///
/// In comparison, a [`Mutex`](crate::sync::Mutex)
/// does not distinguish between readers or writers
/// that acquire the lock, therefore blocking any tasks waiting for the lock to
/// become available. An `RwLock` will allow any number of readers to acquire the
/// lock as long as a writer is not holding the lock.
///
/// The type parameter `T` represents the data that this lock protects. It is
/// required that `T` satisfies [`Sync`] to allow concurrent access through readers. The RAII guards
/// returned from the locking methods implement [`Deref`] (and [`DerefMut`]
/// for the `write` methods) to allow access to the content of the lock.
///
/// # The difference between `RWLock` and [`LocalRWLock`](crate::sync::LocalRWLock)
///
/// The `RWLock` works with `global tasks` and can be shared between threads.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// # Example
///
/// ```no_run
/// use std::collections::HashMap;
/// use orengine::Local;
/// use orengine::sync::RWLock;
///
/// # async fn write_to_the_dump_file(key: usize, value: usize) {}
///
/// async fn dump_storage(storage: Local<RWLock<HashMap<usize, usize>>>) {
///     let mut read_guard = storage.read().await;
///     
///     for (key, value) in read_guard.iter() {
///         write_to_the_dump_file(*key, *value).await;
///     }
///
///     // read lock is released when `guard` goes out of scope
/// }
/// ```
pub struct RWLock<T> {
    number_of_readers: CachePadded<AtomicIsize>,
    value: UnsafeCell<T>,
}

impl<T> RWLock<T> {
    /// Creates a new `RWLock` with the given value.
    #[inline(always)]
    pub fn new(value: T) -> RWLock<T> {
        RWLock {
            number_of_readers: CachePadded::new(AtomicIsize::new(0)),
            value: UnsafeCell::new(value),
        }
    }

    /// Returns [`WriteLockGuard`] that allows mutable access to the inner value.
    ///
    /// It will block the current task if the lock is already acquired for writing or reading.
    #[inline(always)]
    pub async fn write(&self) -> WriteLockGuard<T> {
        loop {
            match self.try_write() {
                Some(guard) => return guard,
                None => yield_now().await,
            }
        }
    }

    /// Returns [`ReadLockGuard`] that allows shared access to the inner value.
    ///
    /// It will block the current task if the lock is already acquired for writing.
    #[inline(always)]
    pub async fn read(&self) -> ReadLockGuard<T> {
        loop {
            match self.try_read() {
                Some(guard) => return guard,
                None => yield_now().await,
            }
        }
    }

    /// If the lock is not acquired for writing or reading, returns [`WriteLockGuard`] that
    /// allows mutable access to the inner value, otherwise returns [`None`].
    #[inline(always)]
    pub fn try_write(&self) -> Option<WriteLockGuard<T>> {
        let was_swapped = self
            .number_of_readers
            .compare_exchange(0, -1, Acquire, Relaxed)
            .is_ok();
        if !was_swapped {
            None
        } else {
            Some(WriteLockGuard::new(self))
        }
    }

    /// If the lock is not acquired for writing, returns [`ReadLockGuard`] that
    /// allows shared access to the inner value, otherwise returns [`None`].
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

    /// Returns a mutable reference to the inner value. It is safe because it uses `&mut self`.
    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    /// Releases the read portion lock.
    ///
    /// # Safety
    ///
    /// - The rw_lock must be read-locked.
    #[inline(always)]
    pub unsafe fn read_unlock(&self) {
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

    /// Releases the write portion lock.
    ///
    /// # Safety
    ///
    /// - The rw_lock must be write-locked.
    #[inline(always)]
    pub unsafe fn write_unlock(&self) {
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

    /// Returns a reference to the inner value.
    ///
    /// # Safety
    ///
    /// - The mutex must be read-locked.
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

    /// Returns a mutable reference to the inner value.
    ///
    /// # Safety
    ///
    /// - The mutex must be write-locked.
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
    use crate as orengine;

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
