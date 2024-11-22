//! This module provides an asynchronous `rw_lock` (e.g. [`std::sync::RwLock`]) type [`RWLock`].
//!
//! It allows for asynchronous read or write locking and unlocking, and provides
//! ownership-based locking through [`ReadLockGuard`] and [`WriteLockGuard`].
use std::cell::UnsafeCell;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};

use crate::sync::{AsyncRWLock, AsyncReadLockGuard, AsyncWriteLockGuard, LockStatus};
use crate::yield_now;
use crossbeam::utils::CachePadded;

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
}

impl<'rw_lock, T: ?Sized> AsyncReadLockGuard<'rw_lock, T> for ReadLockGuard<'rw_lock, T> {
    type RWLock = RWLock<T>;

    fn rw_lock(&self) -> &'rw_lock Self::RWLock {
        self.rw_lock
    }

    #[inline(always)]
    unsafe fn leak(self) -> &'rw_lock Self::RWLock {
        #[allow(
            clippy::missing_transmute_annotations,
            reason = "It is not possible to write Dst"
        )]
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
}

impl<'rw_lock, T: ?Sized> AsyncWriteLockGuard<'rw_lock, T> for WriteLockGuard<'rw_lock, T> {
    type RWLock = RWLock<T>;

    fn rw_lock(&self) -> &'rw_lock Self::RWLock {
        self.rw_lock
    }

    #[inline(always)]
    unsafe fn leak(self) -> &'rw_lock Self::RWLock {
        #[allow(
            clippy::missing_transmute_annotations,
            reason = "It is not possible to write Dst"
        )]
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

/// An asynchronous version of a [`reader-writer lock`](std::sync::RwLock).
///
/// This type of lock allows a number of readers or at most one writer at any
/// point in time. The write portion of this lock typically allows modification
/// of the underlying data (exclusive access) and the read portion of this lock
/// typically allows for read-only access (shared access).
///
/// In comparison, a [`Mutex`](crate::sync::Mutex)
/// does not distinguish between readers or writers
/// that acquire the lock, therefore blocking any tasks waiting for the lock to
/// become available. An `RWLock` will allow any number of readers to acquire the
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
/// use orengine::sync::{AsyncRWLock, RWLock};
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
}

impl<T: ?Sized> AsyncRWLock<T> for RWLock<T> {
    type ReadLockGuard<'rw_lock> = ReadLockGuard<'rw_lock, T>
    where
        T: 'rw_lock,
        Self: 'rw_lock;
    type WriteLockGuard<'rw_lock> = WriteLockGuard<'rw_lock, T>
    where
        T: 'rw_lock,
        Self: 'rw_lock;

    #[inline(always)]
    fn get_lock_status(&self) -> LockStatus {
        match self.number_of_readers.load(Acquire) {
            0 => LockStatus::Unlocked,
            n if n > 0 => LockStatus::ReadLocked(n as usize),
            _ => LockStatus::WriteLocked,
        }
    }

    #[inline(always)]
    async fn write<'rw_lock>(&'rw_lock self) -> Self::WriteLockGuard<'rw_lock>
    where
        T: 'rw_lock,
    {
        loop {
            match self.try_write() {
                Some(guard) => return guard,
                None => yield_now().await,
            }
        }
    }

    #[inline(always)]
    async fn read<'rw_lock>(&'rw_lock self) -> Self::ReadLockGuard<'rw_lock>
    where
        T: 'rw_lock,
    {
        loop {
            match self.try_read() {
                Some(guard) => return guard,
                None => yield_now().await,
            }
        }
    }

    #[inline(always)]
    fn try_write(&self) -> Option<Self::WriteLockGuard<'_>> {
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

    #[inline(always)]
    fn try_read(&self) -> Option<Self::ReadLockGuard<'_>> {
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
    fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    #[inline(always)]
    unsafe fn read_unlock(&self) {
        if cfg!(debug_assertions) {
            let current = self.number_of_readers.load(Acquire);
            assert_ne!(current, 0, "RWLock is already unlocked");
            assert!(current > 0, "RWLock is locked for write");
        }

        self.number_of_readers.fetch_sub(1, Release);
    }

    #[inline(always)]
    unsafe fn write_unlock(&self) {
        if cfg!(debug_assertions) {
            let current = self.number_of_readers.load(Acquire);
            assert_ne!(current, 0, "RWLock is already unlocked");
            assert!(current < 0, "RWLock is locked for read");
        }

        self.number_of_readers.store(0, Release);
    }

    #[inline(always)]
    unsafe fn get_read_locked(&self) -> Self::ReadLockGuard<'_> {
        if cfg!(debug_assertions) {
            let current = self.number_of_readers.load(Acquire);
            assert_ne!(current, 0, "RWLock is already unlocked");
            assert!(current > 0, "RWLock is locked for write");
        }

        ReadLockGuard::new(self)
    }

    #[inline(always)]
    unsafe fn get_write_locked(&self) -> Self::WriteLockGuard<'_> {
        if cfg!(debug_assertions) {
            let current = self.number_of_readers.load(Acquire);
            assert_ne!(current, 0, "RWLock is already unlocked");
            assert!(current < 0, "RWLock is locked for read");
        }

        WriteLockGuard::new(self)
    }
}

unsafe impl<T: ?Sized + Send> Sync for RWLock<T> {}
unsafe impl<T: ?Sized + Send> Send for RWLock<T> {}

/// ```compile_fail
/// use orengine::sync::{RWLock, AsyncRWLock};
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// struct NonSend {
///     value: i32,
///     // impl !Send
///     no_send_marker: std::marker::PhantomData<*const ()>,
/// }
///
/// async fn test() {
///     let mutex = RWLock::new(NonSend {
///         value: 0,
///         no_send_marker: std::marker::PhantomData,
///     });
///
///     let guard = check_send(mutex.read()).await;
///     yield_now().await;
///     assert_eq!(guard.value, 0);
///     drop(guard);
/// }
/// ```
///
/// ```rust
/// use orengine::sync::{RWLock, AsyncRWLock};
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// // impl Send
/// struct CanSend {
///     value: i32,
/// }
///
/// async fn test() {
///     let mutex = RWLock::new(CanSend {
///         value: 0,
///     });
///
///     let guard = check_send(mutex.read()).await;
///     yield_now().await;
///     assert_eq!(guard.value, 0);
///     drop(guard);
/// }
/// ```
///
/// ```compile_fail
/// use orengine::sync::{RWLock, AsyncRWLock};
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// struct NonSend {
///     value: i32,
///     // impl !Send
///     no_send_marker: std::marker::PhantomData<*const ()>,
/// }
///
/// async fn test() {
///     let mutex = RWLock::new(NonSend {
///         value: 0,
///         no_send_marker: std::marker::PhantomData,
///     });
///
///     let guard = check_send(mutex.write()).await;
///     yield_now().await;
///     assert_eq!(guard.value, 0);
///     drop(guard);
/// }
/// ```
///
/// ```rust
/// use orengine::sync::{RWLock, AsyncRWLock};
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// // impl Send
/// struct CanSend {
///     value: i32,
/// }
///
/// async fn test() {
///     let mutex = RWLock::new(CanSend {
///         value: 0,
///     });
///
///     let guard = check_send(mutex.write()).await;
///     yield_now().await;
///     assert_eq!(guard.value, 0);
///     drop(guard);
/// }
/// ```
#[allow(dead_code, reason = "It is used only in compile tests")]
fn test_compile_shared_rw_lock() {}

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
                    drop(lock);
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
                    drop(lock);
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
