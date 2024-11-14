use std::cell::UnsafeCell;
use std::hint::spin_loop;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};

use crate::sync::{AsyncMutex, AsyncMutexGuard};
use crate::yield_now;
use crossbeam::utils::CachePadded;

/// An RAII implementation of a "scoped lock" of a mutex. When this structure is
/// dropped (falls out of scope), the lock will be unlocked.
///
/// The data protected by the mutex can be accessed through this guard via its
/// [`Deref`](Deref) and [`DerefMut`] implementations.
///
/// This structure is created by the [`lock`](NaiveMutex::lock)
/// and [`try_lock`](NaiveMutex::try_lock) methods on [`NaiveMutex`].
pub struct NaiveMutexGuard<'mutex, T: ?Sized> {
    mutex: &'mutex NaiveMutex<T>,
}

impl<'mutex, T: ?Sized> NaiveMutexGuard<'mutex, T> {
    /// Creates a new [`NaiveMutexGuard`].
    #[inline(always)]
    pub(crate) fn new(mutex: &'mutex NaiveMutex<T>) -> Self {
        Self { mutex }
    }

    /// Returns a reference to the [`CachePadded<AtomicBool>`]
    /// associated with the original [`NaiveMutex`] to
    /// use [`Executor::release_atomic_bool`](crate::Executor::release_atomic_bool).
    ///
    /// # Safety
    ///
    /// The mutex is locked now and will be unlocked by calling [`NaiveMutex::unlock`] or
    /// [`Executor::release_atomic_bool`](crate::Executor::release_atomic_bool) later.
    #[inline(always)]
    pub unsafe fn leak_to_atomic(self) -> &'static CachePadded<AtomicBool> {
        debug_assert!(self.mutex.is_locked.load(Acquire));
        let static_mutex = unsafe {
            mem::transmute::<
                &CachePadded<AtomicBool>,
                &'static CachePadded<AtomicBool>
            >(&self.mutex.is_locked)
        };
        mem::forget(self);

        static_mutex
    }
}

impl<'mutex, T: ?Sized> AsyncMutexGuard<'mutex, T> for NaiveMutexGuard<'mutex, T> {
    type Mutex = NaiveMutex<T>;

    fn mutex(&self) -> &'mutex Self::Mutex {
        self.mutex
    }

    unsafe fn leak(self) -> &'mutex Self::Mutex {
        #[allow(clippy::missing_transmute_annotations, reason = "It is not possible to write Dst")]
        let static_mutex = unsafe { mem::transmute(self.mutex) };
        mem::forget(self);

        static_mutex
    }
}

impl<'mutex, T: ?Sized> Deref for NaiveMutexGuard<'mutex, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mutex.value.get() }
    }
}

impl<'mutex, T: ?Sized> DerefMut for NaiveMutexGuard<'mutex, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.value.get() }
    }
}

impl<'mutex, T: ?Sized> Drop for NaiveMutexGuard<'mutex, T> {
    fn drop(&mut self) {
        unsafe { self.mutex.unlock() };
    }
}

unsafe impl<T: ?Sized + Send + Sync> Sync for NaiveMutexGuard<'_, T> {}
unsafe impl<T: ?Sized + Send> Send for NaiveMutexGuard<'_, T> {}

/// A mutual exclusion primitive useful for protecting shared data.
///
/// This mutex will block tasks waiting for the lock to become available. The
/// mutex can be created via a [`new`](NaiveMutex::new) constructor. Each mutex has a type parameter
/// which represents the data that it is protecting. The data can be accessed
/// through the RAII guards returned from [`lock`](NaiveMutex::lock) and
/// [`try_lock`](NaiveMutex::try_lock),
/// which guarantees that the data is only ever accessed when the mutex is locked, or
/// with an unsafe method [`get_locked`](NaiveMutex::get_locked).
///
/// # The difference between `NaiveMutex` and [`LocalMutex`](crate::sync::LocalMutex)
///
/// The `NaiveMutex` works with `shared tasks` and can be shared between threads.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// # The differences between `NaiveMutex` and [`Mutex`](crate::sync::Mutex)
///
/// The [`NaiveMutex`](NaiveMutex) yields the current task if it is unable
/// to acquire the lock.
///
/// The `Mutex` uses a queue of tasks waiting for the lock to become available.
///
/// Use `Mutex` when a lot of tasks are waiting for the same lock because the lock is acquired
/// for a __long__ time. If a lot of tasks are waiting for the same lock because the lock
/// is acquired for a __short__ time try to share the `Mutex`.
///
/// If the lock is mostly acquired the first time, it is better to
/// use [`NaiveMutex`](NaiveMutex), as it spends less time on successful operations.
///
/// # Example
///
/// ```rust
/// use std::collections::HashMap;
/// use orengine::sync::{AsyncMutex, NaiveMutex};
///
/// # async fn write_to_the_dump_file(key: usize, value: usize) {}
///
/// async fn dump_storage(storage: &NaiveMutex<HashMap<usize, usize>>) {
///     let mut guard = storage.lock().await;
///
///     for (key, value) in guard.iter() {
///         write_to_the_dump_file(*key, *value).await;
///     }
///
///     // lock is released when `guard` goes out of scope
/// }
/// ```
pub struct NaiveMutex<T: ?Sized> {
    is_locked: CachePadded<AtomicBool>,
    value: UnsafeCell<T>,
}

impl<T: ?Sized> NaiveMutex<T> {
    /// Creates a new [`NaiveMutex`].
    pub const fn new(value: T) -> Self
    where
        T: Sized,
    {
        Self {
            is_locked: CachePadded::new(AtomicBool::new(false)),
            value: UnsafeCell::new(value),
        }
    }
}

impl<T: ?Sized> AsyncMutex<T> for NaiveMutex<T> {
    type Guard<'mutex> = NaiveMutexGuard<'mutex, T>
    where
        Self: 'mutex;

    #[inline(always)]
    fn is_locked(&self) -> bool {
        self.is_locked.load(Acquire)
    }

    #[inline(always)]
    async fn lock<'mutex>(&'mutex self) -> Self::Guard<'mutex>
    where
        T: 'mutex,
    {
        loop {
            for step in 0..=6 {
                if let Some(guard) = self.try_lock() {
                    return guard;
                }

                for _ in 0..1 << step {
                    spin_loop();
                }
            }

            yield_now().await;
        }
    }

    #[inline(always)]
    fn try_lock(&self) -> Option<Self::Guard<'_>> {
        if self
            .is_locked
            .compare_exchange_weak(false, true, Acquire, Relaxed)
            .is_ok()
        {
            Some(NaiveMutexGuard::new(self))
        } else {
            None
        }
    }

    #[inline(always)]
    fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    #[inline(always)]
    unsafe fn unlock(&self) {
        debug_assert!(
            self.is_locked.load(Acquire),
            "NaiveMutex is unlocked, but calling unlock it must be locked"
        );

        self.is_locked.store(false, Release);
    }

    #[inline(always)]
    #[allow(clippy::mut_from_ref, reason = "The caller guarantees safety using this code")]
    unsafe fn get_locked(&self) -> Self::Guard<'_> {
        debug_assert!(
            self.is_locked.load(Acquire),
            "NaiveMutex is unlocked, but calling get_locked it must be locked"
        );

        Self::Guard::new(self)
    }
}

unsafe impl<T: ?Sized + Send + Sync> Sync for NaiveMutex<T> {}
impl<T: UnwindSafe> UnwindSafe for NaiveMutex<T> {}
impl<T: RefUnwindSafe> RefUnwindSafe for NaiveMutex<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::sleep;
    use crate::sync::WaitGroup;
    use crate::test::sched_future_to_another_thread;
    use std::sync::Arc;
    use std::time::Duration;

    #[orengine_macros::test_shared]
    fn test_naive_mutex() {
        const SLEEP_DURATION: Duration = Duration::from_millis(1);

        let mutex = Arc::new(NaiveMutex::new(false));
        let wg = Arc::new(WaitGroup::new());

        let mutex_clone = mutex.clone();
        let wg_clone = wg.clone();
        wg_clone.add(1);
        sched_future_to_another_thread(async move {
            let mut value = mutex_clone.lock().await;
            println!("1");
            sleep(SLEEP_DURATION).await;
            wg_clone.done();
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

    #[orengine_macros::test_shared]
    fn test_try_naive_mutex() {
        let mutex = Arc::new(NaiveMutex::new(false));
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
        });

        lock_wg.wait().await;
        println!("2");
        let value = mutex.try_lock();
        println!("3");
        assert!(value.is_none());
        second_lock.inc();
        unlock_wg.done();

        second_lock.wait().await;
        let value = mutex.try_lock();
        println!("5");
        match value {
            Some(v) => assert!(*v, "not waited"),
            None => panic!("can't acquire lock"),
        }
    }

    #[orengine_macros::test_shared]
    fn stress_test_naive_mutex() {
        const PAR: usize = 10;
        const TRIES: usize = 100;

        async fn work_with_lock(mutex: &NaiveMutex<usize>, wg: &WaitGroup) {
            let mut lock = mutex.lock().await;
            *lock += 1;
            if *lock % 500 == 0 {
                println!("{} of {}", *lock, TRIES * PAR);
            }

            wg.done();
        }

        let mutex = Arc::new(NaiveMutex::new(0));
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
