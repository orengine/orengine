//! This module provides an asynchronous mutex (e.g. [`std::sync::Mutex`]) type [`Mutex`].
//! It allows for asynchronous locking and unlocking, and provides
//! ownership-based locking through [`MutexGuard`].
use std::cell::{Cell, UnsafeCell};
use std::future::Future;
use std::hint::spin_loop;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::task::{Context, Poll};

use crossbeam::utils::{Backoff, CachePadded};

use crate::panic_if_local_in_future;
use crate::runtime::{local_executor, Task};
use crate::sync_task_queue::SyncTaskList;

/// An RAII implementation of a "scoped lock" of a mutex. When this structure is
/// dropped (falls out of scope), the lock will be unlocked.
///
/// The data protected by the mutex can be accessed through this guard via its
/// [`Deref`](Deref) and [`DerefMut`] implementations.
///
/// This structure is created by the [`lock`](Mutex::lock)
/// and [`try_lock`](Mutex::try_lock) methods on [`Mutex`].
pub struct MutexGuard<'mutex, T> {
    mutex: &'mutex Mutex<T>,
}

impl<'mutex, T> MutexGuard<'mutex, T> {
    /// Creates a new [`MutexGuard`].
    #[inline(always)]
    pub(crate) fn new(mutex: &'mutex Mutex<T>) -> Self {
        Self { mutex }
    }

    /// Returns a reference to the original [`Mutex`].
    #[inline(always)]
    pub fn mutex(&self) -> &Mutex<T> {
        &self.mutex
    }

    /// Unlocks the [`mutex`](Mutex). Calling `guard.unlock()` is equivalent to
    /// calling `drop(guard)`. This was done to improve readability.
    ///
    /// # Attention
    ///
    /// Even if you doesn't call `guard.unlock()`,
    /// the [`mutex`](Mutex) will be unlocked after the `guard` is dropped.
    #[inline(always)]
    pub fn unlock(self) {}

    /// Returns a reference to the original [`Mutex`].
    ///
    /// The mutex will be unlocked.
    #[inline(always)]
    pub(crate) fn into_mutex(self) -> &'mutex Mutex<T> {
        self.mutex
    }

    /// Returns a reference to the original [`Mutex`].
    ///
    /// The mutex will never be unlocked.
    ///
    /// # Safety
    ///
    /// The mutex is unlocked by calling [`Mutex::unlock`] later.
    #[inline(always)]
    pub unsafe fn leak(self) -> &'static Mutex<T> {
        let static_mutex = unsafe { mem::transmute(self.mutex) };
        mem::forget(self);

        static_mutex
    }
}

impl<'mutex, T> Deref for MutexGuard<'mutex, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mutex.value.get() }
    }
}

impl<'mutex, T> DerefMut for MutexGuard<'mutex, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.value.get() }
    }
}

impl<'mutex, T> Drop for MutexGuard<'mutex, T> {
    fn drop(&mut self) {
        unsafe { self.mutex.unlock() };
    }
}

/// `MutexWait` is a future that will be resolved when the lock is acquired.
pub struct MutexWait<'mutex, T> {
    was_called: bool,
    mutex: &'mutex Mutex<T>,
}

impl<'mutex, T> MutexWait<'mutex, T> {
    /// Creates a new [`MutexWait`].
    #[inline(always)]
    fn new(local_mutex: &'mutex Mutex<T>) -> Self {
        Self {
            was_called: false,
            mutex: local_mutex,
        }
    }
}

impl<'mutex, T> Future for MutexWait<'mutex, T> {
    type Output = MutexGuard<'mutex, T>;

    #[allow(unused)] // because #[cfg(debug_assertions)]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        panic_if_local_in_future!(cx, "Mutex");

        if !this.was_called {
            if let Some(guard) = this.mutex.try_lock_with_spinning() {
                return Poll::Ready(guard);
            }

            if this.mutex.counter.fetch_add(1, Acquire) == 0 {
                return Poll::Ready(MutexGuard::new(&this.mutex));
            }

            this.was_called = true;
            unsafe { local_executor().push_current_task_to(&this.mutex.wait_queue) };

            Poll::Pending
        } else {
            Poll::Ready(MutexGuard::new(&this.mutex))
        }
    }
}

/// A mutual exclusion primitive useful for protecting shared data.
///
/// This mutex will block tasks waiting for the lock to become available. The
/// mutex can be created via a [`new`](Mutex::new) constructor. Each mutex has a type parameter
/// which represents the data that it is protecting. The data can be accessed
/// through the RAII guards returned from [`lock`](Mutex::lock) and [`try_lock`](Mutex::try_lock),
/// which guarantees that the data is only ever accessed when the mutex is locked, or
/// with an unsafe method [`get_locked`](Mutex::get_locked).
///
/// # The difference between `Mutex` and [`LocalMutex`](crate::sync::LocalMutex)
///
/// The `Mutex` works with `global tasks` and can be shared between threads.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// # The differences between `Mutex` and [`NaiveMutex`](crate::sync::NaiveMutex)
///
/// The `Mutex` uses a queue of tasks waiting for the lock to become available.
///
/// The [`NaiveMutex`](crate::sync::NaiveMutex) yields the current task if it is unable
/// to acquire the lock.
///
/// Use `Mutex` when a lot of tasks are waiting for the same lock because the lock is acquired
/// for a __long__ time. If a lot of tasks are waiting for the same lock because the lock
/// is acquired for a __short__ time try to share the `Mutex`.
///
/// If the lock is mostly acquired the first time, it is better to
/// use [`NaiveMutex`](crate::sync::NaiveMutex), as it spends less time on successful operations.
///
/// # Example
///
/// ```no_run
/// use std::collections::HashMap;
/// use orengine::sync::Mutex;
///
/// # async fn write_to_the_dump_file(key: usize, value: usize) {}
///
/// async fn dump_storage(storage: &Mutex<HashMap<usize, usize>>) {
///     let mut guard = storage.lock().await;
///
///     for (key, value) in guard.iter() {
///         write_to_the_dump_file(*key, *value).await;
///     }
///
///     // lock is released when `guard` goes out of scope
/// }
/// ```
pub struct Mutex<T> {
    counter: CachePadded<AtomicUsize>,
    wait_queue: SyncTaskList,
    value: UnsafeCell<T>,
    expected_count: Cell<usize>,
}

impl<T> Mutex<T> {
    /// Creates a new [`Mutex`].
    #[inline(always)]
    pub fn new(value: T) -> Mutex<T> {
        Mutex {
            counter: CachePadded::new(AtomicUsize::new(0)),
            wait_queue: SyncTaskList::new(),
            value: UnsafeCell::new(value),
            expected_count: Cell::new(1),
        }
    }

    /// Returns a [`Future`] that resolves to [`MutexGuard`] that allows access to the inner value.
    ///
    /// It blocks the current task if the mutex is locked.
    #[inline(always)]
    pub fn lock(&self) -> MutexWait<T> {
        MutexWait::new(self)
    }

    /// If the mutex is unlocked, returns [`MutexGuard`] that allows access to the inner value,
    /// otherwise returns [`None`].
    ///
    /// # The difference between `try_lock` and [`try_lock_with_spinning`](Mutex::try_lock_with_spinning)
    ///
    /// `try_lock` tries to acquire the lock only once.
    /// It can be more useful in cases where the lock is likely to be locked for
    /// __more than 30 nanoseconds__.
    #[inline(always)]
    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        if self
            .counter
            .compare_exchange(0, 1, Acquire, Relaxed)
            .is_ok()
        {
            Some(MutexGuard::new(self))
        } else {
            None
        }
    }

    /// If the mutex is unlocked, returns [`MutexGuard`] that allows access to the inner value,
    /// otherwise returns [`None`].
    ///
    /// # The difference between `try_lock_with_spinning` and [`try_lock`](Mutex::try_lock)
    ///
    /// `try_lock_with_spinning` tries to acquire the lock in a loop with a small delay a few times.
    /// It can be more useful in cases where the lock is very likely to be locked for
    /// __less than 30 nanoseconds__.
    #[inline(always)]
    pub fn try_lock_with_spinning(&self) -> Option<MutexGuard<T>> {
        for step in 0..=6 {
            let lock_res = self.counter.compare_exchange(0, 1, Acquire, Acquire);
            match lock_res {
                Ok(_) => return Some(MutexGuard::new(self)),
                Err(count) if count == 1 => {
                    for _ in 0..1 << step {
                        spin_loop();
                    }
                }
                Err(_) => return None,
            }
        }

        None
    }

    /// Returns the inner value. It is safe because it uses `&mut self`.
    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    /// Add current task to wait queue.
    ///
    /// # Safety
    ///
    /// Called by owner of the lock of this [`Mutex`].
    #[inline(always)]
    pub(crate) unsafe fn subscribe(&self, task: Task) {
        debug_assert!(self.counter.load(Acquire) != 1, "Mutex is unlocked");
        self.expected_count.set(self.expected_count.get() - 1);
        unsafe {
            self.wait_queue.push(task);
        }
    }

    /// Unlocks the mutex.
    ///
    /// # Safety
    ///
    /// - The mutex must be locked.
    ///
    /// - And no tasks has an ownership of this [`mutex`](Mutex).
    #[inline(always)]
    pub unsafe fn unlock(&self) {
        debug_assert!(self.counter.load(Acquire) != 0, "Mutex is already unlocked");
        // fast path
        let was_swapped = self
            .counter
            .compare_exchange(self.expected_count.get(), 0, Release, Relaxed)
            .is_ok();
        if was_swapped {
            self.expected_count.set(1);
            return;
        }

        self.expected_count.set(self.expected_count.get() + 1);
        let next = self.wait_queue.pop();
        if next.is_some() {
            unsafe { local_executor().exec_task(next.unwrap_unchecked()) };
        } else {
            // Another task failed to acquire a lock, but it is not yet in the queue
            let backoff = Backoff::new();
            loop {
                backoff.spin();
                let next = self.wait_queue.pop();
                if next.is_some() {
                    unsafe { local_executor().exec_task(next.unwrap_unchecked()) };
                    break;
                }
            }
        }
    }

    /// Returns a reference to the inner value.
    ///
    /// # Safety
    ///
    /// - The mutex must be locked.
    ///
    /// - And only current task has an ownership of this [`mutex`](Mutex).
    #[inline(always)]
    pub unsafe fn get_locked(&self) -> &T {
        debug_assert!(
            self.counter.load(Acquire) != 0,
            "Mutex is unlocked, but calling get_locked it must be locked"
        );

        unsafe { &*self.value.get() }
    }
}

unsafe impl<T: Send + Sync> Sync for Mutex<T> {}
unsafe impl<T: Send> Send for Mutex<T> {}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use crate::sleep::sleep;
    use crate::sync::WaitGroup;

    use super::*;
    use crate as orengine;
    use crate::test::sched_future_to_another_thread;

    #[orengine_macros::test_global]
    fn test_global_mutex() {
        const SLEEP_DURATION: Duration = Duration::from_millis(1);

        let mutex = Arc::new(Mutex::new(false));
        let wg = Arc::new(WaitGroup::new());

        let mutex_clone = mutex.clone();
        let wg_clone = wg.clone();
        wg_clone.add(1);
        sched_future_to_another_thread(async move {
            let mut value = mutex_clone.lock().await;
            wg_clone.done();
            println!("1");
            sleep(SLEEP_DURATION).await;
            println!("3");
            *value = true;
        });

        let _ = wg.wait().await;
        println!("2");
        let value = mutex.lock().await;
        println!("4");

        assert_eq!(*value, true);
        drop(value);
    }

    async fn test_try_mutex<F: Fn(&Mutex<bool>) -> Option<MutexGuard<'_, bool>>>(try_lock: F) {
        let mutex = Arc::new(Mutex::new(false));
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
            let mut value = mutex_clone.lock().await;
            println!("1");
            lock_wg_clone.done();
            let _ = unlock_wg_clone.wait().await;
            println!("4");
            *value = true;
            drop(value);
            second_lock_clone.done();
            println!("5");
        });

        let _ = lock_wg.wait().await;
        println!("2");
        let value = try_lock(&mutex);
        println!("3");
        assert!(value.is_none());
        second_lock.inc();
        unlock_wg.done();

        let _ = second_lock.wait().await;
        let value = try_lock(&mutex);
        println!("6");
        match value {
            Some(v) => assert_eq!(*v, true, "not waited"),
            None => panic!("can't acquire lock"),
        }
    }

    #[orengine_macros::test_global]
    fn test_try_without_spinning_global_mutex() {
        test_try_mutex(Mutex::try_lock).await;
    }

    #[orengine_macros::test_global]
    fn test_try_with_spinning_global_mutex() {
        test_try_mutex(Mutex::try_lock_with_spinning).await;
    }

    #[orengine_macros::test_global]
    fn stress_test_global_mutex() {
        const PAR: usize = 5;
        const TRIES: usize = 400;

        async fn work_with_lock(mutex: &Mutex<usize>, wg: &WaitGroup) {
            let mut lock = mutex.lock().await;
            *lock += 1;
            if *lock % 500 == 0 {
                println!("{} of {}", *lock, TRIES * PAR);
            }

            wg.done();
        }

        let mutex = Arc::new(Mutex::new(0));
        let wg = Arc::new(WaitGroup::new());
        wg.add(PAR * TRIES);
        for _ in 1..PAR {
            let wg = wg.clone();
            let mutex = mutex.clone();
            sched_future_to_another_thread(async move {
                for _ in 0..TRIES {
                    work_with_lock(&mutex, &wg).await;
                }
            });
        }

        for _ in 0..TRIES {
            work_with_lock(&mutex, &wg).await;
        }

        let _ = wg.wait().await;

        assert_eq!(*mutex.lock().await, TRIES * PAR);
    }
}
