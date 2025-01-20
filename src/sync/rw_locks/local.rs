//! This module provides an asynchronous `read-write lock` (e.g. [`std::sync::RwLock`])
//! type [`LocalRWLock`].
//!
//! It allows for asynchronous read or write locking and unlocking, and provides
//! ownership-based locking through [`LocalReadLockGuard`] and [`LocalWriteLockGuard`].
use crate::get_task_from_context;
use crate::runtime::local_executor;
use crate::sync::{AsyncRWLock, AsyncReadLockGuard, AsyncWriteLockGuard, LockStatus};
use crate::utils::{acquire_task_vec_from_pool, TaskVecFromPool};
use std::cell::UnsafeCell;
use std::future::Future;
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::task::{Context, Poll};

// region guards

/// RAII structure used to release the shared read access of a lock when
/// dropped.
///
/// This structure is created by the [`LocalRWLock::read`](LocalRWLock::read)
/// and [`LocalRWLock::try_read`](LocalRWLock::try_read).
pub struct LocalReadLockGuard<'rw_lock, T: ?Sized> {
    local_rw_lock: &'rw_lock LocalRWLock<T>,
    // impl !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

impl<'rw_lock, T: ?Sized> LocalReadLockGuard<'rw_lock, T> {
    /// Creates a new `LocalReadLockGuard`.
    #[inline]
    fn new(local_rw_lock: &'rw_lock LocalRWLock<T>) -> Self {
        Self {
            local_rw_lock,
            no_send_marker: std::marker::PhantomData,
        }
    }
}

impl<'rw_lock, T: ?Sized> AsyncReadLockGuard<'rw_lock, T> for LocalReadLockGuard<'rw_lock, T> {
    type RWLock = LocalRWLock<T>;

    fn rw_lock(&self) -> &'rw_lock Self::RWLock {
        self.local_rw_lock
    }

    #[inline]
    unsafe fn leak(self) -> &'rw_lock Self::RWLock {
        ManuallyDrop::new(self).local_rw_lock
    }
}

impl<T: ?Sized> Deref for LocalReadLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.local_rw_lock.get_inner().value
    }
}

impl<T: ?Sized> Drop for LocalReadLockGuard<'_, T> {
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
pub struct LocalWriteLockGuard<'rw_lock, T: ?Sized> {
    local_rw_lock: &'rw_lock LocalRWLock<T>,
    // impl !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

impl<'rw_lock, T: ?Sized> LocalWriteLockGuard<'rw_lock, T> {
    /// Creates a new `LocalWriteLockGuard`.
    #[inline]
    fn new(local_rw_lock: &'rw_lock LocalRWLock<T>) -> Self {
        Self {
            local_rw_lock,
            no_send_marker: std::marker::PhantomData,
        }
    }
}

impl<'rw_lock, T: ?Sized> AsyncWriteLockGuard<'rw_lock, T> for LocalWriteLockGuard<'rw_lock, T> {
    type RWLock = LocalRWLock<T>;

    fn rw_lock(&self) -> &'rw_lock Self::RWLock {
        self.local_rw_lock
    }

    #[inline]
    unsafe fn leak(self) -> &'rw_lock Self::RWLock {
        ManuallyDrop::new(self).local_rw_lock
    }
}

impl<T: ?Sized> Deref for LocalWriteLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.local_rw_lock.get_inner().value
    }
}

impl<T: ?Sized> DerefMut for LocalWriteLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.local_rw_lock.get_inner().value
    }
}

impl<T: ?Sized> Drop for LocalWriteLockGuard<'_, T> {
    fn drop(&mut self) {
        unsafe {
            self.local_rw_lock.write_unlock();
        }
    }
}

// endregion

// region futures

/// `ReadLockWait` is a future that will be resolved when the read lock is acquired.
#[repr(C)]
pub struct ReadLockWait<'rw_lock, T: ?Sized> {
    was_called: bool,
    local_rw_lock: &'rw_lock LocalRWLock<T>,
    // impl !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

impl<'rw_lock, T: ?Sized> ReadLockWait<'rw_lock, T> {
    /// Creates a new `ReadLockWait`.
    #[inline]
    fn new(local_rw_lock: &'rw_lock LocalRWLock<T>) -> Self {
        Self {
            was_called: false,
            local_rw_lock,
            no_send_marker: std::marker::PhantomData,
        }
    }
}

impl<'rw_lock, T: ?Sized> Future for ReadLockWait<'rw_lock, T> {
    type Output = LocalReadLockGuard<'rw_lock, T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = &mut *self;
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
#[repr(C)]
pub struct WriteLockWait<'rw_lock, T: ?Sized> {
    was_called: bool,
    local_rw_lock: &'rw_lock LocalRWLock<T>,
    // impl !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

impl<'rw_lock, T: ?Sized> WriteLockWait<'rw_lock, T> {
    /// Creates a new `WriteLockWait`.
    #[inline]
    fn new(local_rw_lock: &'rw_lock LocalRWLock<T>) -> Self {
        Self {
            was_called: false,
            local_rw_lock,
            no_send_marker: std::marker::PhantomData,
        }
    }
}

impl<'rw_lock, T: ?Sized> Future for WriteLockWait<'rw_lock, T> {
    type Output = LocalWriteLockGuard<'rw_lock, T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = &mut *self;
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
#[repr(C)]
struct Inner<T: ?Sized> {
    wait_queue_read: TaskVecFromPool,
    wait_queue_write: TaskVecFromPool,
    number_of_readers: isize,
    value: T,
}

/// An asynchronous version of a [`reader-writer lock`](std::sync::RwLock)
///
/// This type of lock allows a number of readers or at most one writer at any
/// point in time. The write portion of this lock typically allows modification
/// of the underlying data (exclusive access) and the read portion of this lock
/// typically allows for read-only access (shared access).
///
/// In comparison, a [`LocalMutex`](crate::sync::LocalMutex)
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
/// use orengine::sync::{AsyncRWLock, LocalRWLock};
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
///     *counter.borrow_mut() += 1;
/// }
/// ```
///
/// # Example with correct usage
///
/// ```rust
/// use std::collections::HashMap;
/// use std::rc::Rc;
/// use orengine::sync::{AsyncRWLock, LocalRWLock};
///
/// # async fn write_to_the_dump_file(key: usize, value: usize) {}
///
/// // Correct usage, because after `write_to_log_file(*key, *value).await` and before the future is resolved
/// // another task can modify the storage. So, we need to lock the storage.
/// async fn dump_storage(storage: Rc<LocalRWLock<HashMap<usize, usize>>>) {
///     let mut read_guard = storage.read().await;
///     
///     for (key, value) in read_guard.iter() {
///         write_to_the_dump_file(*key, *value).await;
///     }
///
///     // read lock is released when `guard` goes out of scope
/// }
/// ```
pub struct LocalRWLock<T: ?Sized> {
    // impl !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
    inner: UnsafeCell<Inner<T>>,
}

impl<T: ?Sized> LocalRWLock<T> {
    /// Creates a new `LocalRWLock`.
    pub fn new(value: T) -> Self
    where
        T: Sized,
    {
        Self {
            inner: UnsafeCell::new(Inner {
                wait_queue_read: acquire_task_vec_from_pool(),
                wait_queue_write: acquire_task_vec_from_pool(),
                number_of_readers: 0,
                value,
            }),
            no_send_marker: std::marker::PhantomData,
        }
    }

    /// Returns a mutable reference to the inner value.
    #[inline]
    #[allow(clippy::mut_from_ref, reason = "It is Sync and `local`")]
    fn get_inner(&self) -> &mut Inner<T> {
        unsafe { &mut *self.inner.get() }
    }
}

impl<T: ?Sized> AsyncRWLock<T> for LocalRWLock<T> {
    type ReadLockGuard<'rw_lock>
        = LocalReadLockGuard<'rw_lock, T>
    where
        T: 'rw_lock,
        Self: 'rw_lock;
    type WriteLockGuard<'rw_lock>
        = LocalWriteLockGuard<'rw_lock, T>
    where
        T: 'rw_lock,
        Self: 'rw_lock;

    #[inline]
    fn get_lock_status(&self) -> LockStatus {
        #[allow(clippy::cast_sign_loss, reason = "false positive")]
        match self.get_inner().number_of_readers {
            0 => LockStatus::Unlocked,
            n if n > 0 => LockStatus::ReadLocked(n as usize),
            _ => LockStatus::WriteLocked,
        }
    }

    #[inline]
    #[allow(clippy::future_not_send, reason = "Because it is `local`")]
    async fn write<'rw_lock>(&'rw_lock self) -> Self::WriteLockGuard<'rw_lock>
    where
        T: 'rw_lock,
    {
        let inner = self.get_inner();

        if inner.number_of_readers == 0 {
            debug_assert!(inner.wait_queue_read.is_empty());

            inner.number_of_readers = -1;
            return LocalWriteLockGuard::new(self);
        }

        WriteLockWait::new(self).await
    }

    #[inline]
    #[allow(clippy::future_not_send, reason = "Because it is `local`")]
    async fn read<'rw_lock>(&'rw_lock self) -> Self::ReadLockGuard<'rw_lock>
    where
        T: 'rw_lock,
    {
        let inner = self.get_inner();

        if inner.number_of_readers > -1 {
            inner.number_of_readers += 1;
            return LocalReadLockGuard::new(self);
        }

        ReadLockWait::new(self).await
    }

    #[inline]
    fn try_write(&self) -> Option<Self::WriteLockGuard<'_>> {
        let inner = self.get_inner();

        if inner.number_of_readers == 0 {
            debug_assert!(inner.wait_queue_read.is_empty());

            inner.number_of_readers = -1;
            Some(LocalWriteLockGuard::new(self))
        } else {
            None
        }
    }

    #[inline]
    fn try_read(&self) -> Option<Self::ReadLockGuard<'_>> {
        let inner = self.get_inner();
        if inner.number_of_readers > -1 {
            inner.number_of_readers += 1;
            Some(LocalReadLockGuard::new(self))
        } else {
            None
        }
    }

    #[inline]
    fn get_mut(&mut self) -> &mut T {
        &mut self.inner.get_mut().value
    }

    #[inline]
    unsafe fn read_unlock(&self) {
        if cfg!(debug_assertions) {
            assert_ne!(
                self.get_inner().number_of_readers,
                -1,
                "LocalRWLock is locked for write"
            );

            assert_ne!(
                self.get_inner().number_of_readers,
                0,
                "LocalRWLock is already unlocked"
            );
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

    #[inline]
    unsafe fn write_unlock(&self) {
        if cfg!(debug_assertions) {
            assert_ne!(
                self.get_inner().number_of_readers,
                0,
                "LocalRWLock is already unlocked"
            );

            assert!(
                self.get_inner().number_of_readers <= 0,
                "LocalRWLock is locked for read"
            );
        }

        let inner = self.get_inner();

        let task = inner.wait_queue_write.pop();
        if task.is_none() {
            let mut readers_count = inner.wait_queue_read.len();

            #[allow(clippy::cast_possible_wrap, reason = "false positive")]
            {
                inner.number_of_readers = readers_count as isize;
            }

            while readers_count > 0 {
                let task = inner.wait_queue_read.pop();
                local_executor().exec_task(unsafe { task.unwrap_unchecked() });
                readers_count -= 1;
            }
        } else {
            local_executor().exec_task(unsafe { task.unwrap_unchecked() });
        }
    }

    #[inline]
    unsafe fn get_read_locked(&self) -> Self::ReadLockGuard<'_> {
        if cfg!(debug_assertions) {
            assert_ne!(
                self.get_inner().number_of_readers,
                -1,
                "LocalRWLock is locked for write"
            );

            assert_ne!(
                self.get_inner().number_of_readers,
                0,
                "LocalRWLock is unlocked"
            );
        }

        LocalReadLockGuard::new(self)
    }

    #[inline]
    unsafe fn get_write_locked(&self) -> Self::WriteLockGuard<'_> {
        if cfg!(debug_assertions) {
            assert_ne!(
                self.get_inner().number_of_readers,
                0,
                "LocalRWLock is unlocked, but get_write_locked is called"
            );

            assert!(
                self.get_inner().number_of_readers <= 0,
                "LocalRWLock is locked for read"
            );
        }

        LocalWriteLockGuard::new(self)
    }
}

unsafe impl<T: ?Sized + Sync> Sync for LocalRWLock<T> {}

/// ```compile_fail
/// use orengine::sync::{LocalRWLock, AsyncRWLock};
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// async fn test() {
///     let mutex = LocalRWLock::new(0);
///
///     let guard = check_send(mutex.read()).await;
///     yield_now().await;
///     assert_eq!(*guard, 0);
///     drop(guard);
/// }
/// ```
///
/// ```compile_fail
/// use orengine::sync::{LocalRWLock, AsyncRWLock};
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// async fn test() {
///     let mutex = LocalRWLock::new(0);
///
///     let guard = check_send(mutex.write()).await;
///     yield_now().await;
///     assert_eq!(*guard, 0);
///     drop(guard);
/// }
/// ```
#[allow(dead_code, reason = "It is used only in compile tests")]
fn test_compile_local_rw_lock() {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::sync::{AsyncWaitGroup, LocalWaitGroup};
    use crate::yield_now;
    use std::rc::Rc;

    #[orengine::test::test_local]
    fn test_local_rw_lock() {
        let rw_lock = Rc::new(LocalRWLock::new(0));
        let wg = Rc::new(LocalWaitGroup::new());
        let read_wg = Rc::new(LocalWaitGroup::new());

        for i in 1..=15 {
            let mutex = rw_lock.clone();
            local_executor().exec_local_future(async move {
                let value = mutex.read().await;
                assert_eq!(mutex.get_inner().number_of_readers, i);
                assert_eq!(*value, 0);
                yield_now().await;
                assert_eq!(mutex.get_inner().number_of_readers, 16 - i);
                assert_eq!(*value, 0);
            });
        }

        for _ in 1..=15 {
            let wg = wg.clone();
            let read_wg = read_wg.clone();
            wg.add(1);
            let mutex = rw_lock.clone();
            local_executor().exec_local_future(async move {
                assert_eq!(mutex.get_inner().number_of_readers, 15);
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
        assert_eq!(*value, 15);
        assert_ne!(rw_lock.get_inner().number_of_readers, 0);
    }

    #[orengine::test::test_local]
    fn test_try_local_rw_lock() {
        const NUMBER_OF_READERS: isize = 5;
        let rw_lock = Rc::new(LocalRWLock::new(0));

        for i in 1..=NUMBER_OF_READERS {
            let mutex = rw_lock.clone();
            local_executor().exec_local_future(async move {
                let lock = mutex.try_read().expect("Failed to get read lock!");
                assert_eq!(mutex.get_inner().number_of_readers, i);
                yield_now().await;
                drop(lock);
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
