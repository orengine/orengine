use std::cell::UnsafeCell;
use std::hint::spin_loop;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};

use crossbeam::utils::{CachePadded};

use crate::yield_now;

pub struct NaiveMutexGuard<'mutex, T> {
    mutex: &'mutex NaiveMutex<T>,
}

impl<'mutex, T> NaiveMutexGuard<'mutex, T> {
    #[inline(always)]
    pub(crate) fn new(mutex: &'mutex NaiveMutex<T>) -> Self {
        Self { mutex }
    }

    #[inline(always)]
    pub fn mutex(&self) -> &NaiveMutex<T> {
        &self.mutex
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
    pub unsafe fn leak(self) -> &'mutex T {
        let static_mutex = unsafe { mem::transmute(self.mutex) };
        mem::forget(self);

        static_mutex
    }

    #[inline(always)]
    pub(crate) unsafe fn leak_to_atomic(self) -> &'mutex CachePadded<AtomicBool> {
        let static_mutex = unsafe { mem::transmute(&self.mutex.is_locked) };
        mem::forget(self);

        static_mutex
    }
}

impl<'mutex, T> Deref for NaiveMutexGuard<'mutex, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mutex.value.get() }
    }
}

impl<'mutex, T> DerefMut for NaiveMutexGuard<'mutex, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.value.get() }
    }
}

impl<'mutex, T> Drop for NaiveMutexGuard<'mutex, T> {
    fn drop(&mut self) {
        unsafe { self.mutex.unlock() };
    }
}

pub struct NaiveMutex<T> {
    is_locked: CachePadded<AtomicBool>,
    value: UnsafeCell<T>,
}

impl<T> NaiveMutex<T> {
    #[inline(always)]
    pub fn new(value: T) -> NaiveMutex<T> {
        NaiveMutex {
            is_locked: CachePadded::new(AtomicBool::new(false)),
            value: UnsafeCell::new(value),
        }
    }

    #[inline(always)]
    pub async fn lock(&self) -> NaiveMutexGuard<T> {
        loop {
            for step in 0..=6 {
                if let Some(guard) = self.try_lock() {
                    return guard;
                }
                for _ in 0..1 <<step {
                    spin_loop();
                }
            }

            yield_now().await;
        }
    }

    #[inline(always)]
    pub fn try_lock(&self) -> Option<NaiveMutexGuard<T>> {
        if self
            .is_locked
            .compare_exchange(false, true, Acquire, Relaxed)
            .is_ok()
        {
            Some(NaiveMutexGuard::new(self))
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    #[inline(always)]
    pub unsafe fn unlock(&self) {
        debug_assert!(
            self.is_locked.load(Acquire),
            "NaiveMutex is unlocked, but calling unlock it must be locked"
        );

        self.is_locked.store(false, Release);
    }

    #[inline(always)]
    pub unsafe fn get_locked(&self) -> &mut T {
        debug_assert!(
            self.is_locked.load(Acquire),
            "Mutex is unlocked, but calling get_locked it must be locked"
        );

        &mut *self.value.get()
    }
}

unsafe impl<T: Send> Sync for NaiveMutex<T> {}
unsafe impl<T: Send> Send for NaiveMutex<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;
    use crate::{sleep, Executor};
    use crate::sync::WaitGroup;

    #[orengine_macros::test]
    fn test_naive_mutex() {
        const SLEEP_DURATION: Duration = Duration::from_millis(1);

        let mutex = Arc::new(NaiveMutex::new(false));
        let wg = Arc::new(WaitGroup::new());

        let mutex_clone = mutex.clone();
        let wg_clone = wg.clone();
        wg_clone.add(1);
        thread::spawn(move || {
            let ex = Executor::init();
            let _ = ex.run_with_future(async move {
                let mut value = mutex_clone.lock().await;
                println!("1");
                sleep(SLEEP_DURATION).await;
                wg_clone.done();
                println!("3");
                *value = true;
            });
        });

        let _ = wg.wait().await;
        println!("2");
        let value = mutex.lock().await;
        println!("4");

        assert_eq!(*value, true);
        drop(value);
    }

    #[orengine_macros::test]
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
        thread::spawn(move || {
            let ex = Executor::init();
            let _ = ex.run_with_future(async move {
                let mut value = mutex_clone.lock().await;
                println!("1");
                lock_wg_clone.done();
                let _ = unlock_wg_clone.wait().await;
                println!("4");
                *value = true;
                drop(value);
                second_lock_clone.done();
            });
        });

        let _ = lock_wg.wait().await;
        println!("2");
        let value = mutex.try_lock();
        println!("3");
        assert!(value.is_none());
        second_lock.inc();
        unlock_wg.done();

        let _ = second_lock.wait().await;
        let value = mutex.try_lock();
        println!("5");
        match value {
            Some(v) => assert_eq!(*v, true, "not waited"),
            None => panic!("can't acquire lock"),
        }
    }

    #[orengine_macros::test]
    fn stress_test_naive_mutex() {
        const PAR: usize = 50;
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
            thread::spawn(move || {
                let ex = Executor::init();
                let _ = ex.run_and_block_on(async move {
                    for _ in 0..TRIES {
                        work_with_lock(&mutex, &wg).await;
                    }
                });
            });
        }

        for _ in 0..TRIES {
            work_with_lock(&mutex, &wg).await;
        }

        let _ = wg.wait().await;

        assert_eq!(*mutex.lock().await, TRIES * PAR);
    }
}