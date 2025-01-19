//! This module provides an asynchronous mutex (e.g. [`std::sync::Mutex`]) type [`Mutex`].
//! It allows for asynchronous locking and unlocking, and provides
//! ownership-based locking through [`MutexGuard`].
use std::cell::{Cell, UnsafeCell};
use std::future::Future;
use std::hint::spin_loop;
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::task::{Context, Poll};

use crossbeam::utils::{Backoff, CachePadded};

use crate::runtime::call::Call;
use crate::runtime::local_executor;
use crate::sync::mutexes::AsyncSubscribableMutex;
use crate::sync::{AsyncMutex, AsyncMutexGuard};
use crate::utils::{acquire_sync_task_list_from_pool, SyncTaskListFromPool};
use crate::{get_task_from_context, panic_if_local_in_future};

/// An RAII implementation of a "scoped lock" of a mutex. When this structure is
/// dropped (falls out of scope), the lock will be unlocked.
///
/// The data protected by the mutex can be accessed through this guard via its
/// [`Deref`] and [`DerefMut`] implementations.
///
/// This structure is created by the [`lock`](Mutex::lock)
/// and [`try_lock`](Mutex::try_lock) methods on [`Mutex`].
pub struct MutexGuard<'mutex, T: ?Sized> {
    mutex: &'mutex Mutex<T>,
}

impl<'mutex, T: ?Sized> MutexGuard<'mutex, T> {
    /// Creates a new [`MutexGuard`].
    #[inline]
    pub(crate) fn new(mutex: &'mutex Mutex<T>) -> Self {
        Self { mutex }
    }
}

impl<'mutex, T: ?Sized> AsyncMutexGuard<'mutex, T> for MutexGuard<'mutex, T> {
    type Mutex = Mutex<T>;

    fn mutex(&self) -> &'mutex Self::Mutex {
        self.mutex
    }

    unsafe fn leak(self) -> &'mutex Self::Mutex {
        ManuallyDrop::new(self).mutex
    }
}

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mutex.value.get() }
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.value.get() }
    }
}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        unsafe { self.mutex.unlock() };
    }
}

unsafe impl<T: ?Sized + Send + Sync> Sync for MutexGuard<'_, T> {}
unsafe impl<T: ?Sized + Send> Send for MutexGuard<'_, T> {}

/// `MutexWait` is a future that will be resolved when the lock is acquired.
#[repr(C)]
pub struct MutexWait<'mutex, T: ?Sized> {
    was_called: bool,
    mutex: &'mutex Mutex<T>,
}

impl<'mutex, T: ?Sized> MutexWait<'mutex, T> {
    /// Creates a new [`MutexWait`].
    #[inline]
    fn new(local_mutex: &'mutex Mutex<T>) -> Self {
        Self {
            was_called: false,
            mutex: local_mutex,
        }
    }
}

impl<'mutex, T: ?Sized> Future for MutexWait<'mutex, T> {
    type Output = MutexGuard<'mutex, T>;

    #[allow(unused, reason = "Here we use #[cfg(debug_assertions)].")]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        unsafe { panic_if_local_in_future!(cx, "Mutex") };

        if !this.was_called {
            if let Some(guard) = this.mutex.try_lock_with_spinning() {
                return Poll::Ready(guard);
            }

            if this.mutex.counter.fetch_add(1, Acquire) == 0 {
                return Poll::Ready(MutexGuard::new(this.mutex));
            }

            this.was_called = true;
            unsafe {
                local_executor().invoke_call(Call::PushCurrentTaskTo(&*this.mutex.wait_queue));
            };

            Poll::Pending
        } else {
            Poll::Ready(MutexGuard::new(this.mutex))
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
/// The `Mutex` works with `shared tasks` and can be shared between threads.
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
/// ```rust
/// use std::collections::HashMap;
/// use orengine::sync::{AsyncMutex, Mutex};
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
#[repr(C)]
pub struct Mutex<T: ?Sized> {
    counter: CachePadded<AtomicUsize>,
    wait_queue: SyncTaskListFromPool,
    expected_count: Cell<usize>,
    value: UnsafeCell<T>,
}

impl<T: ?Sized> Mutex<T> {
    /// Creates a new [`Mutex`].
    pub fn new(value: T) -> Self
    where
        T: Sized,
    {
        Self {
            counter: CachePadded::new(AtomicUsize::new(0)),
            wait_queue: acquire_sync_task_list_from_pool(),
            value: UnsafeCell::new(value),
            expected_count: Cell::new(1),
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
    #[inline]
    pub fn try_lock_with_spinning(&self) -> Option<MutexGuard<T>> {
        for step in 0..=6 {
            let lock_res = self.counter.compare_exchange(0, 1, Acquire, Acquire);
            return match lock_res {
                Ok(_) => Some(MutexGuard::new(self)),
                Err(count) => {
                    if count == 1 {
                        for _ in 0..1 << step {
                            spin_loop();
                        }

                        continue;
                    }

                    None
                }
            };
        }

        None
    }
}

impl<T: ?Sized> AsyncMutex<T> for Mutex<T> {
    type Guard<'mutex>
        = MutexGuard<'mutex, T>
    where
        Self: 'mutex;

    #[inline]
    fn is_locked(&self) -> bool {
        self.counter.load(Acquire) != 0
    }

    #[inline]
    #[allow(
        clippy::future_not_send,
        reason = "It is not `Send` only when T is not `Send`, it is fine"
    )]
    fn lock<'mutex>(&'mutex self) -> impl Future<Output = Self::Guard<'mutex>>
    where
        T: 'mutex,
    {
        MutexWait::new(self)
    }

    #[inline]
    fn try_lock(&self) -> Option<Self::Guard<'_>> {
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

    #[inline]
    fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    #[inline]
    unsafe fn unlock(&self) {
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

    #[inline]
    unsafe fn get_locked(&self) -> Self::Guard<'_> {
        debug_assert!(
            self.counter.load(Acquire) != 0,
            "Mutex is unlocked, but calling get_locked it must be locked"
        );

        Self::Guard::new(self)
    }
}

impl<T: ?Sized> AsyncSubscribableMutex<T> for Mutex<T> {
    #[inline]
    fn low_level_subscribe(&self, cx: &Context) {
        let task = unsafe { get_task_from_context!(cx) };

        self.expected_count.set(self.expected_count.get() - 1);
        unsafe {
            self.wait_queue.push(task);
        }
    }
}

unsafe impl<T: ?Sized + Send + Sync> Sync for Mutex<T> {}
impl<T: ?Sized + UnwindSafe> UnwindSafe for Mutex<T> {}
impl<T: ?Sized + RefUnwindSafe> RefUnwindSafe for Mutex<T> {}

/// ```compile_fail
/// use orengine::sync::{Mutex, AsyncMutex};
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
///     let mutex = Mutex::new(NonSend {
///         value: 0,
///         no_send_marker: std::marker::PhantomData,
///     });
///
///     let guard = check_send(mutex.lock()).await;
///     yield_now().await;
///     assert_eq!(guard.value, 0);
///     drop(guard);
/// }
/// ```
///
/// ```rust
/// use orengine::sync::{Mutex, AsyncMutex};
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
///     let mutex = Mutex::new(CanSend {
///         value: 0,
///     });
///
///     let guard = check_send(mutex.lock()).await;
///     yield_now().await;
///     assert_eq!(guard.value, 0);
///     drop(guard);
/// }
/// ```
#[allow(dead_code, reason = "It is used only in compile tests")]
fn test_compile_smart_mutex() {}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use crate::sleep::sleep;
    use crate::sync::{AsyncWaitGroup, WaitGroup};

    use super::*;
    use crate as orengine;
    use crate::test::sched_future_to_another_thread;

    #[orengine::test::test_shared]
    fn test_shared_mutex() {
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

        wg.wait().await;
        println!("2");
        let value = mutex.lock().await;
        println!("4");

        assert!(*value);
        drop(value);
    }

    async fn test_try_mutex<F>(try_lock: F)
    where
        F: Send + Fn(&Mutex<bool>) -> Option<MutexGuard<'_, bool>>,
    {
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
            unlock_wg_clone.wait().await;
            println!("4");
            *value = true;
            drop(value);
            second_lock_clone.done();
            println!("5");
        });

        lock_wg.wait().await;
        println!("2");
        let value = try_lock(&mutex);
        println!("3");
        assert!(value.is_none());
        second_lock.inc();
        unlock_wg.done();

        second_lock.wait().await;
        let value = try_lock(&mutex);
        println!("6");
        match value {
            Some(v) => assert!(*v, "not waited"),
            None => panic!("can't acquire lock"),
        }
    }

    #[orengine::test::test_shared]
    fn test_try_without_spinning_shared_mutex() {
        test_try_mutex(Mutex::try_lock).await;
    }

    #[orengine::test::test_shared]
    fn test_try_with_spinning_shared_mutex() {
        test_try_mutex(Mutex::try_lock_with_spinning).await;
    }

    #[orengine::test::test_shared]
    fn stress_test_shared_mutex() {
        const PAR: usize = 5;
        const TRIES: usize = 400;

        async fn work_with_lock(mutex: &Mutex<usize>, wg: &WaitGroup) {
            let mut lock = mutex.lock().await;
            *lock += 1;

            wg.done();
        }

        for _ in 0..20 {
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

            wg.wait().await;

            assert_eq!(*mutex.lock().await, TRIES * PAR);
        }
    }
}
