//! This module provides an asynchronous `rw_lock` (e.g. [`std::sync::RwLock`]) type [`RWLock`].
//!
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
pub struct ReadLockGuard<'rw_lock, T: ?Sized> {
    rw_lock: &'rw_lock RWLock<T>,
}

impl<'rw_lock, T: ?Sized> ReadLockGuard<'rw_lock, T> {
    /// Creates a new `ReadLockGuard`.
    #[inline(always)]
    fn new(rw_lock: &'rw_lock RWLock<T>) -> Self {
        Self { rw_lock }
    }

    /// Returns a reference to the original [`RWLock`].
    #[inline(always)]
    pub fn rw_lock(&self) -> &RWLock<T> {
        self.rw_lock
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
    /// The [`RWLock`] is read unlocked by calling [`RWLock::read_unlock`] later.
    #[inline(always)]
    pub unsafe fn leak(self) -> &'rw_lock T {
        #[allow(clippy::missing_transmute_annotations, reason = "It is not possible to write Dst")]
        #[allow(
            clippy::transmute_ptr_to_ptr,
            reason = "It is not possible to write it any other way"
        )]
        let static_mutex = unsafe { mem::transmute(self.rw_lock) };
        mem::forget(self);

        static_mutex
    }
}

impl<'rw_lock, T: ?Sized> Deref for ReadLockGuard<'rw_lock, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.rw_lock.value.get() }
    }
}

impl<'rw_lock, T: ?Sized> Drop for ReadLockGuard<'rw_lock, T> {
    fn drop(&mut self) {
        unsafe {
            self.rw_lock.read_unlock();
        }
    }
}

unsafe impl<T: ?Sized + Send + Sync> Sync for ReadLockGuard<'_, T> {}
unsafe impl<T: ?Sized + Send> Send for ReadLockGuard<'_, T> {}

/// RAII structure used to release the exclusive write access of a lock when
/// dropped.
///
/// This structure is created by the [`RWLock::write`](RWLock::write)
/// and [`RWLock::try_write`](RWLock::try_write).
pub struct WriteLockGuard<'rw_lock, T: ?Sized> {
    rw_lock: &'rw_lock RWLock<T>,
}

impl<'rw_lock, T: ?Sized> WriteLockGuard<'rw_lock, T> {
    /// Creates a new `WriteLockGuard`.
    #[inline(always)]
    fn new(rw_lock: &'rw_lock RWLock<T>) -> Self {
        Self { rw_lock }
    }

    /// Returns a reference to the original [`RWLock`].
    #[inline(always)]
    pub fn rw_lock(&self) -> &RWLock<T> {
        self.rw_lock
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
    /// The [`RWLock`] is write-unlocked by calling [`RWLock::write_unlock`] later.
    #[inline(always)]
    pub unsafe fn leak(self) -> &'rw_lock T {
        #[allow(clippy::missing_transmute_annotations, reason = "It is not possible to write Dst")]
        #[allow(
            clippy::transmute_ptr_to_ptr,
            reason = "It is not possible to write it any other way"
        )]
        let static_mutex = unsafe { mem::transmute(self.rw_lock) };
        mem::forget(self);

        static_mutex
    }
}

impl<'rw_lock, T: ?Sized> Deref for WriteLockGuard<'rw_lock, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.rw_lock.value.get() }
    }
}

impl<'rw_lock, T: ?Sized> DerefMut for WriteLockGuard<'rw_lock, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.rw_lock.value.get() }
    }
}

impl<'rw_lock, T: ?Sized> Drop for WriteLockGuard<'rw_lock, T> {
    fn drop(&mut self) {
        unsafe {
            self.rw_lock.write_unlock();
        }
    }
}

unsafe impl<T: ?Sized + Send + Sync> Sync for WriteLockGuard<'_, T> {}
unsafe impl<T: ?Sized + Send> Send for WriteLockGuard<'_, T> {}

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
/// The `RWLock` works with `shared tasks` and can be shared between threads.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// # Example
///
/// ```rust
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
pub struct RWLock<T: ?Sized> {
    number_of_readers: CachePadded<AtomicIsize>,
    value: UnsafeCell<T>,
}

pub enum LockStatus {
    Unlocked,
    WriteLocked,
    ReadLocked(usize),
}

impl<T: ?Sized> RWLock<T> {
    /// Creates a new `RWLock` with the given value.
    pub const fn new(value: T) -> Self
    where
        T: Sized,
    {
        Self {
            number_of_readers: CachePadded::new(AtomicIsize::new(0)),
            value: UnsafeCell::new(value),
        }
    }

    /// Returns

    /// Returns [`WriteLockGuard`] that allows mutable access to the inner value.
    ///
    /// It will block the current task if the lock is already acquired for writing or reading.
    #[inline(always)]
    pub async fn write(&self) -> WriteLockGuard<T>
    where
        T: Send,
    {
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
    pub async fn read(&self) -> ReadLockGuard<T>
    where
        T: Send,
    {
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
    /// [`RWLock`] is read-locked and leaked via [`ReadLockGuard::leak`].
    #[inline(always)]
    #[allow(clippy::missing_panics_doc, reason = "False positive")]
    pub unsafe fn read_unlock(&self) {
        if cfg!(debug_assertions) {
            let current = self.number_of_readers.load(Acquire);
            assert_ne!(current, 0, "RWLock is already unlocked");
            assert!(current > 0, "RWLock is locked for write");
        }

        self.number_of_readers.fetch_sub(1, Release);
    }

    /// Releases the write portion lock.
    ///
    /// # Safety
    ///
    /// - [`RWLock`] is write-locked and leaked via [`WriteLockGuard::leak`].
    ///
    /// - No other tasks has an ownership of this [`RWLock`].
    #[inline(always)]
    #[allow(clippy::missing_panics_doc, reason = "False positive")]
    pub unsafe fn write_unlock(&self) {
        if cfg!(debug_assertions) {
            let current = self.number_of_readers.load(Acquire);
            assert_ne!(current, 0, "RWLock is already unlocked");
            assert!(current < 0, "RWLock is locked for read");
        }

        self.number_of_readers.store(0, Release);
    }

    /// Returns a reference to the inner value.
    ///
    /// # Safety
    ///
    /// - [`RWLock`] must be read-locked.
    #[inline(always)]
    #[allow(clippy::missing_panics_doc, reason = "False positive")]
    pub unsafe fn get_read_locked(&self) -> &T {
        if cfg!(debug_assertions) {
            let current = self.number_of_readers.load(Acquire);
            assert_ne!(current, 0, "RWLock is already unlocked");
            assert!(current > 0, "RWLock is locked for write");
        }

        unsafe { &*self.value.get() }
    }

    /// Returns a mutable reference to the inner value.
    ///
    /// # Safety
    ///
    /// - [`RWLock`] must be write-locked
    ///
    /// - No tasks has an ownership of this [`RWLock`].
    #[inline(always)]
    #[allow(clippy::mut_from_ref, reason = "The caller guarantees safety using this code")]
    #[allow(clippy::missing_panics_doc, reason = "False positive")]
    pub unsafe fn get_write_locked(&self) -> &mut T {
        if cfg!(debug_assertions) {
            let current = self.number_of_readers.load(Acquire);
            assert_ne!(current, 0, "RWLock is already unlocked");
            assert!(current < 0, "RWLock is locked for read");
        }

        unsafe { &mut *self.value.get() }
    }
}

unsafe impl<T: ?Sized + Send> Sync for RWLock<T> {}
unsafe impl<T: ?Sized + Send> Send for RWLock<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::sync::shared_scope;
    use std::sync::atomic::Ordering::SeqCst;

    #[orengine_macros::test_shared]
    fn test_naive_rw_lock() {
        const NUMBER_OF_READERS: isize = 5;
        let rw_lock = RWLock::new(0);

        shared_scope(|scope| async {
            for i in 1..=NUMBER_OF_READERS {
                scope.exec(async {
                    let lock = rw_lock.read().await;
                    assert_eq!(rw_lock.number_of_readers.load(SeqCst), i);
                    yield_now().await;
                    lock.unlock();
                });
            }

            assert_eq!(rw_lock.number_of_readers.load(SeqCst), NUMBER_OF_READERS);

            let mut write_lock = rw_lock.write().await;
            assert!(rw_lock.try_write().is_none());
            *write_lock += 1;
            assert_eq!(*write_lock, 1);
            assert_eq!(rw_lock.number_of_readers.load(SeqCst), -1);
        })
            .await;
    }

    #[orengine_macros::test_shared]
    fn test_try_naive_rw_lock() {
        const NUMBER_OF_READERS: isize = 5;
        let rw_lock = RWLock::new(0);

        shared_scope(|scope| async {
            for i in 1..=NUMBER_OF_READERS {
                scope.exec(async {
                    let lock = rw_lock.try_read().expect("Failed to get read lock!");
                    assert_eq!(rw_lock.number_of_readers.load(SeqCst), i);
                    yield_now().await;
                    lock.unlock();
                });
            }

            assert_eq!(rw_lock.number_of_readers.load(SeqCst), NUMBER_OF_READERS);
            assert!(
                rw_lock.try_write().is_none(),
                "Successful attempt to acquire write lock when rw_lock locked for read"
            );

            yield_now().await;

            assert_eq!(rw_lock.number_of_readers.load(SeqCst), 0);
            let mut write_lock = rw_lock.try_write().expect("Failed to get write lock!");
            *write_lock += 1;
            assert_eq!(*write_lock, 1);

            assert_eq!(rw_lock.number_of_readers.load(SeqCst), -1);
            assert!(
                rw_lock.try_read().is_none(),
                "Successful attempt to acquire read lock when rw_lock locked for write"
            );
            assert!(
                rw_lock.try_write().is_none(),
                "Successful attempt to acquire write lock when rw_lock locked for write"
            );
        })
            .await;
    }
}
