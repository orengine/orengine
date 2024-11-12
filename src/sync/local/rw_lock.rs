//! This module provides an asynchronous rw_lock (e.g. [`std::sync::RwLock`]) type [`LocalRWLock`].
//!
//! It allows for asynchronous read or write locking and unlocking, and provides
//! ownership-based locking through [`LocalReadLockGuard`] and [`LocalWriteLockGuard`].
use crate::get_task_from_context;
use crate::runtime::local_executor;
use crate::runtime::task::Task;
use std::cell::UnsafeCell;
use std::future::Future;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::task::{Context, Poll};
// region guards

/// RAII structure used to release the shared read access of a lock when
/// dropped.
///
/// This structure is created by the [`LocalRWLock::read`](LocalRWLock::read)
/// and [`LocalRWLock::try_read`](LocalRWLock::try_read).
pub struct LocalReadLockGuard<'rw_lock, T> {
    local_rw_lock: &'rw_lock LocalRWLock<T>,
    // impl !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

impl<'rw_lock, T> LocalReadLockGuard<'rw_lock, T> {
    /// Creates a new `LocalReadLockGuard`.
    #[inline(always)]
    fn new(local_rw_lock: &'rw_lock LocalRWLock<T>) -> Self {
        Self {
            local_rw_lock,
            no_send_marker: std::marker::PhantomData,
        }
    }

    /// Returns a reference to the original [`LocalRWLock`].
    #[inline(always)]
    pub fn local_rw_lock(&self) -> &LocalRWLock<T> {
        &self.local_rw_lock
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

    /// Returns a reference to the original [`LocalRWLock`].
    ///
    /// The read portion of this lock will not be released.
    ///
    /// # Safety
    ///
    /// The rw_lock is read unlocked by calling [`LocalRWLock::read_unlock`] later.
    #[inline(always)]
    pub unsafe fn leak(self) -> &'static LocalRWLock<T> {
        let static_local_rw_lock = unsafe { mem::transmute(self.local_rw_lock) };
        mem::forget(self);

        static_local_rw_lock
    }
}

impl<'rw_lock, T> Deref for LocalReadLockGuard<'rw_lock, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.local_rw_lock.get_inner().value
    }
}

impl<'rw_lock, T> Drop for LocalReadLockGuard<'rw_lock, T> {
    fn drop(&mut self) {
        unsafe {
            self.local_rw_lock.read_unlock();
        }
    }
}

/// RAII structure used to release the exclusive write access of a lock when
/// dropped.
///
/// This structure is created by the [`LocalRWLock::write`](LocalRWLock::write)
/// and [`LocalRWLock::try_write`](LocalRWLock::try_write).
pub struct LocalWriteLockGuard<'rw_lock, T> {
    local_rw_lock: &'rw_lock LocalRWLock<T>,
    // impl !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

impl<'rw_lock, T> LocalWriteLockGuard<'rw_lock, T> {
    /// Creates a new `LocalWriteLockGuard`.
    #[inline(always)]
    fn new(local_rw_lock: &'rw_lock LocalRWLock<T>) -> Self {
        Self {
            local_rw_lock,
            no_send_marker: std::marker::PhantomData,
        }
    }

    /// Returns a reference to the original [`LocalRWLock`].
    #[inline(always)]
    pub fn local_rw_lock(&self) -> &LocalRWLock<T> {
        &self.local_rw_lock
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

    /// Returns a reference to the original [`LocalRWLock`].
    ///
    /// The write portion of this lock will not be released.
    ///
    /// # Safety
    ///
    /// The rw_lock is write unlocked by calling [`LocalRWLock::write_unlock`] later.
    #[inline(always)]
    pub unsafe fn leak(self) -> &'static LocalRWLock<T> {
        let static_local_rw_lock = unsafe { mem::transmute(self.local_rw_lock) };
        mem::forget(self);

        static_local_rw_lock
    }
}

impl<'rw_lock, T> Deref for LocalWriteLockGuard<'rw_lock, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.local_rw_lock.get_inner().value
    }
}

impl<'rw_lock, T> DerefMut for LocalWriteLockGuard<'rw_lock, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.local_rw_lock.get_inner().value
    }
}

impl<'rw_lock, T> Drop for LocalWriteLockGuard<'rw_lock, T> {
    fn drop(&mut self) {
        unsafe {
            self.local_rw_lock.write_unlock();
        }
    }
}

// endregion

// region futures

/// `ReadLockWait` is a future that will be resolved when the read lock is acquired.
pub struct ReadLockWait<'rw_lock, T> {
    was_called: bool,
    local_rw_lock: &'rw_lock LocalRWLock<T>,
}

impl<'rw_lock, T> ReadLockWait<'rw_lock, T> {
    /// Creates a new `ReadLockWait`.
    #[inline(always)]
    fn new(local_rw_lock: &'rw_lock LocalRWLock<T>) -> Self {
        Self {
            was_called: false,
            local_rw_lock,
        }
    }
}

impl<'rw_lock, T> Future for ReadLockWait<'rw_lock, T> {
    type Output = LocalReadLockGuard<'rw_lock, T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        if !this.was_called {
            let task = unsafe { get_task_from_context!(cx) };
            this.local_rw_lock.get_inner().wait_queue_read.push(task);
            this.was_called = true;
            return Poll::Pending;
        }

        Poll::Ready(LocalReadLockGuard::new(this.local_rw_lock))
    }
}

/// `WriteLockWait` is a future that will be resolved when the write lock is acquired.
pub struct WriteLockWait<'rw_lock, T> {
    was_called: bool,
    local_rw_lock: &'rw_lock LocalRWLock<T>,
}

impl<'rw_lock, T> WriteLockWait<'rw_lock, T> {
    /// Creates a new `WriteLockWait`.
    #[inline(always)]
    fn new(local_rw_lock: &'rw_lock LocalRWLock<T>) -> Self {
        Self {
            was_called: false,
            local_rw_lock,
        }
    }
}

impl<'rw_lock, T> Future for WriteLockWait<'rw_lock, T> {
    type Output = LocalWriteLockGuard<'rw_lock, T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        if !this.was_called {
            let task = unsafe { get_task_from_context!(cx) };
            this.local_rw_lock.get_inner().wait_queue_write.push(task);
            this.was_called = true;
            return Poll::Pending;
        }

        Poll::Ready(LocalWriteLockGuard::new(this.local_rw_lock))
    }
}

// endregion

/// Inner structure of [`LocalRWLock`] for internal use via [`UnsafeCell`].
struct Inner<T> {
    wait_queue_read: Vec<Task>,
    wait_queue_write: Vec<Task>,
    number_of_readers: isize,
    value: T,
}

/// A reader-writer lock.
///
/// This type of lock allows a number of readers or at most one writer at any
/// point in time. The write portion of this lock typically allows modification
/// of the underlying data (exclusive access) and the read portion of this lock
/// typically allows for read-only access (shared access).
///
/// In comparison, a [`LocalMutex`](crate::sync::LocalMutex)
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
/// # The difference between `LocalRWLock` and [`RWLock`](crate::sync::RWLock)
///
/// The `LocalRWLock` works with `local tasks`.
///
/// Read [`Executor`](crate::Executor) for more details.
///
///
/// # Incorrect usage
///
/// ```rust
/// use orengine::sync::LocalRWLock;
///
/// // Incorrect usage, because in local runtime all tasks are executed sequentially.
/// async fn inc_counter(counter: &LocalRWLock<u32>) {
///     let mut guard = counter.write().await;
///     *guard += 1;
/// }
/// ```
///
/// Use [`Local`](crate::Local) instead.
///
/// ```rust
/// use orengine::Local;
///
/// // Correct usage, because in local runtime all tasks are executed sequentially.
/// async fn inc_counter(counter: Local<u32>) {
///     *counter.get_mut() += 1;
/// }
/// ```
///
/// # Example with correct usage
///
/// ```rust
/// use std::collections::HashMap;
/// use orengine::Local;
/// use orengine::sync::LocalRWLock;
///
/// # async fn write_to_the_dump_file(key: usize, value: usize) {}
///
/// // Correct usage, because after `write_to_log_file(*key, *value).await` and before the future is resolved
/// // another task can modify the storage. So, we need to lock the storage.
/// async fn dump_storage(storage: Local<LocalRWLock<HashMap<usize, usize>>>) {
///     let mut read_guard = storage.read().await;
///     
///     for (key, value) in read_guard.iter() {
///         write_to_the_dump_file(*key, *value).await;
///     }
///
///     // read lock is released when `guard` goes out of scope
/// }
/// ```
pub struct LocalRWLock<T> {
    inner: UnsafeCell<Inner<T>>,
    // impl !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

impl<T> LocalRWLock<T> {
    /// Creates a new `LocalRWLock`.
    #[inline(always)]
    pub fn new(value: T) -> LocalRWLock<T> {
        LocalRWLock {
            inner: UnsafeCell::new(Inner {
                wait_queue_read: Vec::new(),
                wait_queue_write: Vec::new(),
                number_of_readers: 0,
                value,
            }),
            no_send_marker: std::marker::PhantomData,
        }
    }

    /// Returns a mutable reference to the inner value.
    #[inline(always)]
    fn get_inner(&self) -> &mut Inner<T> {
        unsafe { &mut *self.inner.get() }
    }

    /// Returns [`LocalWriteLockGuard`] that allows mutable access to the inner value.
    ///
    /// It will block the current task if the lock is already acquired for writing or reading.
    #[inline(always)]
    pub async fn write(&self) -> LocalWriteLockGuard<T> {
        let inner = self.get_inner();

        if inner.number_of_readers == 0 {
            debug_assert!(inner.wait_queue_read.is_empty());

            inner.number_of_readers = -1;
            return LocalWriteLockGuard::new(self);
        }

        WriteLockWait::new(self).await
    }

    /// Returns [`LocalReadLockGuard`] that allows shared access to the inner value.
    ///
    /// It will block the current task if the lock is already acquired for writing.
    #[inline(always)]
    pub async fn read(&self) -> LocalReadLockGuard<T> {
        let inner = self.get_inner();

        if inner.number_of_readers > -1 {
            inner.number_of_readers += 1;
            return LocalReadLockGuard::new(self);
        }

        ReadLockWait::new(self).await
    }

    /// If the lock is not acquired for writing or reading, returns [`LocalWriteLockGuard`] that
    /// allows mutable access to the inner value, otherwise returns [`None`].
    #[inline(always)]
    pub fn try_write(&self) -> Option<LocalWriteLockGuard<T>> {
        let inner = self.get_inner();

        if inner.number_of_readers == 0 {
            debug_assert!(inner.wait_queue_read.is_empty());

            inner.number_of_readers = -1;
            Some(LocalWriteLockGuard::new(self))
        } else {
            None
        }
    }

    /// If the lock is not acquired for writing, returns [`LocalReadLockGuard`] that
    /// allows shared access to the inner value, otherwise returns [`None`].
    #[inline(always)]
    pub fn try_read(&self) -> Option<LocalReadLockGuard<T>> {
        let inner = self.get_inner();
        if inner.number_of_readers > -1 {
            inner.number_of_readers += 1;
            Some(LocalReadLockGuard::new(self))
        } else {
            None
        }
    }

    /// Returns a mutable reference to the inner value. It is safe because it uses `&mut self`.
    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner.get_mut().value
    }

    /// Releases the read portion lock.
    ///
    /// # Safety
    ///
    /// - The rw_lock must be read-locked.
    #[inline(always)]
    pub unsafe fn read_unlock(&self) {
        if cfg!(debug_assertions) {
            if self.get_inner().number_of_readers == -1 {
                panic!("LocalRWLock is locked for write");
            }

            if self.get_inner().number_of_readers == 0 {
                panic!("LocalRWLock is already unlocked");
            }
        }

        let inner = self.get_inner();
        inner.number_of_readers -= 1;

        if inner.number_of_readers == 0 {
            debug_assert!(inner.wait_queue_read.is_empty());
            let task = inner.wait_queue_write.pop();
            if task.is_some() {
                inner.number_of_readers = -1;
                local_executor().exec_task(unsafe { task.unwrap_unchecked() });
            }
        }
    }

    /// Releases the write portion lock.
    ///
    /// # Safety
    ///
    /// - The rw_lock must be write-locked.
    #[inline(always)]
    pub unsafe fn write_unlock(&self) {
        if cfg!(debug_assertions) {
            if self.get_inner().number_of_readers == 0 {
                panic!("LocalRWLock is already unlocked");
            }

            if self.get_inner().number_of_readers > 0 {
                panic!("LocalRWLock is locked for read");
            }
        }

        let inner = self.get_inner();

        let task = inner.wait_queue_write.pop();
        if task.is_none() {
            let mut readers_count = inner.wait_queue_read.len();
            inner.number_of_readers = readers_count as isize;
            while readers_count > 0 {
                let task = inner.wait_queue_read.pop();
                local_executor().exec_task(unsafe { task.unwrap_unchecked() });
                readers_count -= 1;
            }
        } else {
            local_executor().exec_task(unsafe { task.unwrap_unchecked() });
        }
    }

    /// Returns a reference to the inner value.
    ///
    /// # Safety
    ///
    /// - The mutex must be read-locked.
    #[inline(always)]
    pub unsafe fn get_read_locked(&self) -> &T {
        if cfg!(debug_assertions) {
            if self.get_inner().number_of_readers == -1 {
                panic!("LocalRWLock is locked for write");
            }

            if self.get_inner().number_of_readers == 0 {
                panic!("LocalRWLock is unlocked, but get_read_locked is called");
            }
        }

        &self.get_inner().value
    }

    /// Returns a mutable reference to the inner value.
    ///
    /// # Safety
    ///
    /// - The mutex must be write-locked.
    #[inline(always)]
    pub unsafe fn get_write_locked(&self) -> &mut T {
        if cfg!(debug_assertions) {
            if self.get_inner().number_of_readers == 0 {
                panic!("LocalRWLock is unlocked, but get_write_locked is called");
            }

            if self.get_inner().number_of_readers > 0 {
                panic!("LocalRWLock is locked for read");
            }
        }

        &mut self.get_inner().value
    }
}

unsafe impl<T: Sync> Sync for LocalRWLock<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::sync::LocalWaitGroup;
    use crate::yield_now;
    use std::rc::Rc;

    #[orengine_macros::test_local]
    fn test_local_rw_lock() {
        let rw_lock = Rc::new(LocalRWLock::new(0));
        let wg = Rc::new(LocalWaitGroup::new());
        let read_wg = Rc::new(LocalWaitGroup::new());

        for i in 1..=30 {
            let mutex = rw_lock.clone();
            local_executor().exec_local_future(async move {
                let value = mutex.read().await;
                assert_eq!(mutex.get_inner().number_of_readers, i);
                assert_eq!(*value, 0);
                yield_now().await;
                assert_eq!(mutex.get_inner().number_of_readers, 31 - i);
                assert_eq!(*value, 0);
            });
        }

        for _ in 1..=30 {
            let wg = wg.clone();
            let read_wg = read_wg.clone();
            wg.add(1);
            let mutex = rw_lock.clone();
            local_executor().exec_local_future(async move {
                assert_eq!(mutex.get_inner().number_of_readers, 30);
                let mut value = mutex.write().await;
                {
                    let read_wg = read_wg.clone();
                    let mutex = mutex.clone();
                    read_wg.add(1);

                    local_executor().exec_local_future(async move {
                        assert_eq!(mutex.get_inner().number_of_readers, -1);
                        let value = mutex.read().await;
                        assert_ne!(*value, 0);
                        assert_ne!(mutex.get_inner().number_of_readers, 0);
                        read_wg.done();
                    });
                }

                assert_eq!(mutex.get_inner().number_of_readers, -1);
                *value += 1;

                wg.done();
            });
        }

        wg.wait().await;
        read_wg.wait().await;

        let value = rw_lock.read().await;
        assert_eq!(*value, 30);
        assert_ne!(rw_lock.get_inner().number_of_readers, 0);
    }

    #[orengine_macros::test_local]
    fn test_try_local_rw_lock() {
        const NUMBER_OF_READERS: isize = 5;
        let rw_lock = Rc::new(LocalRWLock::new(0));

        for i in 1..=NUMBER_OF_READERS {
            let mutex = rw_lock.clone();
            local_executor().exec_local_future(async move {
                let lock = mutex.try_read().expect("Failed to get read lock!");
                assert_eq!(mutex.get_inner().number_of_readers, i);
                yield_now().await;
                lock.unlock();
            });
        }

        assert_eq!(rw_lock.get_inner().number_of_readers, NUMBER_OF_READERS);
        assert!(
            rw_lock.try_write().is_none(),
            "Successful attempt to acquire write lock when rw_lock locked for read"
        );

        yield_now().await;

        assert_eq!(rw_lock.get_inner().number_of_readers, 0);
        let mut write_lock = rw_lock.try_write().expect("Failed to get write lock!");
        *write_lock += 1;
        assert_eq!(*write_lock, 1);

        assert_eq!(rw_lock.get_inner().number_of_readers, -1);
        assert!(
            rw_lock.try_read().is_none(),
            "Successful attempt to acquire read lock when rw_lock locked for write"
        );
        assert!(
            rw_lock.try_write().is_none(),
            "Successful attempt to acquire write lock when rw_lock locked for write"
        );
    }
}
