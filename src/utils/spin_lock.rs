use std::cell::UnsafeCell;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::sync::atomic;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use crossbeam::utils::{Backoff, CachePadded};

pub struct SpinLockGuard<'spin_lock, T> {
    spin_lock: &'spin_lock SpinLock<T>
}

impl<'spin_lock, T> SpinLockGuard<'spin_lock, T> {
    #[inline(always)]
    pub(crate) fn new(spin_lock: &'spin_lock SpinLock<T>) -> Self {
        Self { spin_lock }
    }

    #[inline(always)]
    pub fn spin_lock(&self) -> &SpinLock<T> {
        &self.spin_lock
    }

    #[inline(always)]
    /// Unlocks the spin_lock. Calling `guard.unlock()` is equivalent to calling `drop(guard)`.
    /// This was done to improve readability.
    ///
    /// # Attention
    ///
    /// Even if you doesn't call `guard.unlock()`,
    /// the spin_lock will be unlocked after the `guard` is dropped.
    pub fn unlock(self) {}

    #[inline(always)]
    pub unsafe fn leak(self) -> *const CachePadded<AtomicBool> {
        let atomic_ptr = &self.spin_lock.is_locked;
        mem::forget(self);

        atomic_ptr
    }
}

impl<'spin_lock, T> Deref for SpinLockGuard<'spin_lock, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.spin_lock.value.get() }
    }
}

impl<'spin_lock, T> DerefMut for SpinLockGuard<'spin_lock, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.spin_lock.value.get() }
    }
}

impl<'spin_lock, T> Drop for SpinLockGuard<'spin_lock, T> {
    fn drop(&mut self) {
        unsafe { self.spin_lock.unlock() };
    }
}

pub struct SpinLock<T> {
    is_locked: CachePadded<AtomicBool>,
    value: UnsafeCell<T>,
}

impl<T> SpinLock<T> {
    #[inline(always)]
    pub const fn new(value: T) -> SpinLock<T> {
        SpinLock {
            is_locked: CachePadded::new(AtomicBool::new(false)),
            value: UnsafeCell::new(value),
        }
    }

    #[inline(always)]
    #[cfg(not(debug_assertions))]
    pub fn lock(&self) -> SpinLockGuard<T> {
        let backoff = Backoff::new();
        loop {
            if let Some(guard) = self.try_lock() {
                atomic::fence(Acquire);
                return guard;
            }
            backoff.spin();
        }
    }

    #[inline(always)]
    #[cfg(debug_assertions)]
    pub fn lock(&self) -> SpinLockGuard<T> {
        let backoff = Backoff::new();
        let start = std::time::Instant::now();
        let mut was_println = false;
        loop {
            if let Some(guard) = self.try_lock() {
                atomic::fence(Acquire);
                return guard;
            }
            backoff.spin();
            if backoff.is_completed() && !was_println {
                let elapsed = start.elapsed();
                if elapsed > std::time::Duration::from_nanos(200) {
                    was_println = true;
                    println!("SpinLock is locked for too long. Use Mutex instead.");
                }
            }
        }
    }

    #[inline(always)]
    pub fn try_lock(&self) -> Option<SpinLockGuard<T>> {
        if self.is_locked.compare_exchange_weak(false, true, Relaxed, Relaxed).is_ok() {
            Some(SpinLockGuard::new(self))
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    #[inline(always)]
    pub unsafe fn get_locked(&self) -> &mut T {
        debug_assert!(self.is_locked.load(Acquire));
        unsafe { &mut *self.value.get() }
    }

    #[inline(always)]
    pub unsafe fn unlock(&self) {
        debug_assert!(self.is_locked.load(Acquire));
        self.is_locked.store(false, Release);
    }
}

unsafe impl<T: Send> Sync for SpinLock<T> {}
unsafe impl<T: Send> Send for SpinLock<T> {}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use crate::sync::WaitGroup;
    use crate::Executor;

    use super::*;

    #[orengine_macros::test_global]
    fn test_try_mutex() {
        let mutex = Arc::new(SpinLock::new(false));
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
            Executor::init().run_with_global_future(async move {
                let mut value = mutex_clone.lock();
                println!("1");
                lock_wg_clone.done();
                let _ = unlock_wg_clone.wait().await;
                println!("4");
                *value = true;
                drop(value);
                second_lock_clone.done();
                println!("5");
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
        println!("6");
        match value {
            Some(v) => assert_eq!(*v, true, "not waited"),
            None => panic!("can't acquire lock"),
        }
    }

    #[orengine_macros::test_global]
    fn stress_test_mutex() {
        const PAR: usize = 50;
        const TRIES: usize = 100;

        async fn work_with_lock(mutex: &SpinLock<usize>, wg: &WaitGroup) {
            let mut lock = mutex.lock();
            *lock += 1;
            if *lock % 500 == 0 {
                println!("{} of {}", *lock, TRIES * PAR);
            }
            lock.unlock();

            wg.done();
        }

        let mutex = Arc::new(SpinLock::new(0));
        let wg = Arc::new(WaitGroup::new());
        wg.add(PAR * TRIES);
        for _ in 1..PAR {
            let wg = wg.clone();
            let mutex = mutex.clone();
            thread::spawn(move || {
                Executor::init().run_with_global_future(async move {
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

        assert_eq!(*mutex.lock(), TRIES * PAR);
    }
}