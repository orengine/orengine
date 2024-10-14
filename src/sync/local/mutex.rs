//! This module provides an asynchronous mutex (e.g. [`std::sync::Mutex`]) type [`LocalMutex`].
//! It allows for asynchronous locking and unlocking, and provides
//! ownership-based locking through [`LocalMutexGuard`].
use crate::get_task_from_context;
use crate::runtime::local_executor;
use crate::runtime::task::Task;
use std::cell::UnsafeCell;
use std::future::Future;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::task::{Context, Poll};

/// An RAII implementation of a "scoped lock" of a mutex. When this structure is
/// dropped (falls out of scope), the lock will be unlocked.
///
/// The data protected by the mutex can be accessed through this guard via its
/// [`Deref`](Deref) and [`DerefMut`] implementations.
///
/// This structure is created by the [`lock`](LocalMutex::lock)
/// and [`try_lock`](LocalMutex::try_lock) methods on [`LocalMutex`].
pub struct LocalMutexGuard<'mutex, T> {
    local_mutex: &'mutex LocalMutex<T>,
    // impl !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

impl<'mutex, T> LocalMutexGuard<'mutex, T> {
    /// Creates a new [`LocalMutexGuard`].
    #[inline(always)]
    pub(crate) fn new(local_mutex: &'mutex LocalMutex<T>) -> Self {
        Self {
            local_mutex,
            no_send_marker: std::marker::PhantomData,
        }
    }

    /// Returns a reference to the original [`LocalMutex`].
    #[inline(always)]
    pub fn local_mutex(&'mutex self) -> &'mutex LocalMutex<T> {
        &self.local_mutex
    }

    /// Unlocks the mutex. Calling `guard.unlock()` is equivalent to calling `drop(guard)`.
    /// This was done to improve readability.
    ///
    /// # Attention
    ///
    /// Even if you doesn't call `guard.unlock()`,
    /// the mutex will be unlocked after the `guard` is dropped.
    #[inline(always)]
    pub fn unlock(self) {}

    /// Returns a reference to the original [`LocalMutex`].
    ///
    /// The mutex will be unlocked.
    #[inline(always)]
    pub fn into_local_mutex(self) -> &'mutex LocalMutex<T> {
        self.local_mutex
    }

    /// Returns a reference to the original [`LocalMutex`].
    ///
    /// The mutex will never be unlocked.
    ///
    /// # Safety
    ///
    /// The mutex is unlocked by calling [`LocalMutex::unlock`](LocalMutex::unlock) later.
    #[inline(always)]
    pub unsafe fn leak(self) -> &'static LocalMutex<T> {
        let static_local_mutex = unsafe { mem::transmute(self.local_mutex) };
        mem::forget(self);

        static_local_mutex
    }
}

impl<'mutex, T> Deref for LocalMutexGuard<'mutex, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.local_mutex.value.get() }
    }
}

impl<'mutex, T> DerefMut for LocalMutexGuard<'mutex, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.local_mutex.value.get() }
    }
}

impl<'mutex, T> Drop for LocalMutexGuard<'mutex, T> {
    fn drop(&mut self) {
        unsafe { self.local_mutex.unlock() };
    }
}

/// `MutexWait` is a future that will be resolved when the lock is acquired.
pub struct MutexWait<'mutex, T> {
    was_called: bool,
    local_mutex: &'mutex LocalMutex<T>,
}

impl<'mutex, T> MutexWait<'mutex, T> {
    /// Creates a new [`MutexWait`].
    #[inline(always)]
    pub fn new(local_mutex: &'mutex LocalMutex<T>) -> Self {
        Self {
            was_called: false,
            local_mutex,
        }
    }
}

impl<'mutex, T> Future for MutexWait<'mutex, T> {
    type Output = LocalMutexGuard<'mutex, T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        if !this.was_called {
            let task = get_task_from_context!(cx);
            let wait_queue = unsafe { &mut *this.local_mutex.wait_queue.get() };
            wait_queue.push(task);
            this.was_called = true;
            return Poll::Pending;
        }

        Poll::Ready(LocalMutexGuard::new(this.local_mutex))
    }
}

/// A mutual exclusion primitive useful for protecting shared data.
///
/// This mutex will block tasks waiting for the lock to become available. The
/// mutex can be created via a [`new`](LocalMutex::new) constructor. Each mutex has a type parameter
/// which represents the data that it is protecting. The data can be accessed
/// through the RAII guards returned from [`lock`](LocalMutex::lock)
/// and [`try_lock`](LocalMutex::try_lock), which
/// guarantees that the data is only ever accessed when the mutex is locked, or
/// with an unsafe method [`get_locked`](LocalMutex::get_locked).
///
/// # The difference between `LocalMutex` and [`Mutex`](crate::sync::Mutex)
///
/// The `LocalMutex` works with `local tasks`.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// # Incorrect usage
///
/// ```no_run
/// use orengine::sync::LocalMutex;
///
/// // Incorrect usage, because in local runtime all tasks are executed sequentially.
/// async fn inc_counter(counter: &LocalMutex<u32>) {
///     let mut guard = counter.lock().await;
///     *guard += 1;
/// }
/// ```
///
/// Use [`Local`](crate::Local) instead.
///
/// ```no_run
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
/// ```no_run
/// use std::collections::HashMap;
/// use orengine::sync::LocalMutex;
///
/// # async fn write_to_the_dump_file(key: usize, value: usize) {}
///
/// // Correct usage, because after `write_to_log_file(*key, *value).await` and before the future is resolved
/// // another task can modify the storage. So, we need to lock the storage.
/// async fn dump_storage(storage: &LocalMutex<HashMap<usize, usize>>) {
///     let mut guard = storage.lock().await;
///     
///     for (key, value) in guard.iter() {
///         write_to_the_dump_file(*key, *value).await;
///     }
///
///     // lock is released when `guard` goes out of scope
/// }
/// ```
pub struct LocalMutex<T> {
    is_locked: UnsafeCell<bool>,
    wait_queue: UnsafeCell<Vec<Task>>,
    value: UnsafeCell<T>,
    // impl !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

impl<T> LocalMutex<T> {
    /// Creates a new [`LocalMutex`].
    #[inline(always)]
    pub fn new(value: T) -> LocalMutex<T> {
        LocalMutex {
            is_locked: UnsafeCell::new(false),
            wait_queue: UnsafeCell::new(Vec::new()),
            value: UnsafeCell::new(value),
            no_send_marker: std::marker::PhantomData,
        }
    }

    /// Returns [`LocalMutexGuard`] that allows access to the inner value.
    ///
    /// It blocks the current task if the mutex is locked.
    #[inline(always)]
    pub async fn lock(&self) -> LocalMutexGuard<T> {
        let is_locked = unsafe { &mut *self.is_locked.get() };
        if !*is_locked {
            *is_locked = true;
            LocalMutexGuard::new(self)
        } else {
            MutexWait::new(self).await
        }
    }

    /// If the mutex is unlocked, returns [`LocalMutexGuard`] that allows access to the inner value,
    /// otherwise returns [`None`].
    #[inline(always)]
    pub fn try_lock(&self) -> Option<LocalMutexGuard<T>> {
        let is_locked = unsafe { &mut *self.is_locked.get() };
        if !*is_locked {
            *is_locked = true;
            Some(LocalMutexGuard::new(self))
        } else {
            None
        }
    }

    /// Returns the inner value. It is safe because it uses `&mut self`.
    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.value.get() }
    }

    /// Add current task to wait queue.
    #[inline(always)]
    pub(crate) fn subscribe(&self, task: Task) {
        let wait_queue = unsafe { &mut *self.wait_queue.get() };
        wait_queue.push(task);
    }

    /// Unlocks the mutex.
    ///
    /// # Safety
    ///
    /// - The mutex must be locked.
    ///
    /// - And no tasks has an ownership of this [`mutex`](LocalMutex).
    #[inline(always)]
    pub unsafe fn unlock(&self) {
        debug_assert!(unsafe { self.is_locked.get().read() });

        let wait_queue = unsafe { &mut *self.wait_queue.get() };
        let next = wait_queue.pop();
        if next.is_some() {
            local_executor().exec_task(unsafe { next.unwrap_unchecked() });
        } else {
            let is_locked = unsafe { &mut *self.is_locked.get() };
            *is_locked = false;
        }
    }

    /// Returns a reference to the inner value.
    ///
    /// # Safety
    ///
    /// - The mutex must be locked.
    ///
    /// - And only current task has an ownership of this [`mutex`](LocalMutex).
    #[inline(always)]
    pub unsafe fn get_locked(&self) -> &mut T {
        debug_assert!(
            unsafe { self.is_locked.get().read() },
            "LocalMutex is unlocked, but calling get_locked it must be locked"
        );

        unsafe { &mut *self.value.get() }
    }
}

unsafe impl<T> Sync for LocalMutex<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sleep::sleep;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    #[orengine_macros::test]
    fn test_mutex() {
        let start = Instant::now();
        const SLEEP_DURATION: Duration = Duration::from_millis(1);

        let mutex = Rc::new(LocalMutex::new(false));
        let mutex_clone = mutex.clone();
        local_executor().exec_local_future(async move {
            let mut value = mutex_clone.lock().await;
            println!("1");
            sleep(SLEEP_DURATION).await;
            println!("3");
            *value = true;
        });

        println!("2");
        let value = mutex.lock().await;
        println!("4");

        let elapsed = start.elapsed();
        assert!(elapsed >= SLEEP_DURATION);
        assert_eq!(*value, true);
    }

    #[orengine_macros::test]
    fn test_try_mutex() {
        const SLEEP_DURATION: Duration = Duration::from_millis(1);

        let start = Instant::now();
        let mutex = Rc::new(LocalMutex::new(false));
        let mutex_clone = mutex.clone();
        local_executor().exec_local_future(async move {
            let mut value = mutex_clone.lock().await;
            println!("1");
            sleep(SLEEP_DURATION).await;
            println!("4");
            *value = true;
        });

        println!("2");
        let value = mutex.try_lock();
        println!("3");
        assert!(value.is_none());

        sleep(SLEEP_DURATION * 2).await;

        let elapsed = start.elapsed();
        assert!(elapsed >= SLEEP_DURATION * 2);
        let value = mutex.try_lock();
        println!("5");
        assert_eq!(*(value.expect("not waited")), true);
    }
}
