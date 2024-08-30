use std::cell::{Cell, UnsafeCell};
use std::future::Future;
use std::intrinsics::likely;
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::ptr;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::task::{Context, Poll};
use crossbeam::utils::{Backoff, CachePadded};
use crate::atomic_task_queue::AtomicTaskList;
use crate::runtime::{local_executor, local_executor_unchecked, Task};

pub struct MutexGuard<'mutex, T> {
    mutex: &'mutex Mutex<T>
}

impl<'mutex, T> MutexGuard<'mutex, T> {
    #[inline(always)]
    pub(crate) fn new(mutex: &'mutex Mutex<T>) -> Self {
        Self { mutex }
    }

    #[inline(always)]
    pub fn mutex(&self) -> &Mutex<T> {
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
    pub(crate) fn into_local_mutex(self) -> Mutex<T> {
        unsafe {
            self.mutex.unlock();
            ptr::read(ManuallyDrop::new(self).mutex)
        }
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

pub struct MutexWait<'mutex, T> {
    was_called: bool,
    mutex: &'mutex Mutex<T>
}

impl<'mutex, T> MutexWait<'mutex, T> {
    #[inline(always)]
    fn new(local_mutex: &'mutex Mutex<T>) -> Self {
        Self {
            was_called: false,
            mutex: local_mutex
        }
    }
}

impl<'mutex, T> Future for MutexWait<'mutex, T> {
    type Output = MutexGuard<'mutex, T>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        if likely(!this.was_called) {
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

pub struct Mutex<T> {
    counter: CachePadded<AtomicUsize>,
    wait_queue: AtomicTaskList,
    value: UnsafeCell<T>,
    expected_count: Cell<usize>
}

impl<T> Mutex<T> {
    #[inline(always)]
    pub fn new(value: T) -> Mutex<T> {
        Mutex {
            counter: CachePadded::new(AtomicUsize::new(0)),
            wait_queue: AtomicTaskList::new(),
            value: UnsafeCell::new(value),
            expected_count: Cell::new(1)
        }
    }

    #[inline(always)]
    pub fn lock(&self) -> MutexWait<T> {
        MutexWait::new(self)
    }

    #[inline(always)]
    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        if self.counter.compare_exchange(0, 1, Acquire, Relaxed).is_ok() {
            Some(MutexGuard::new(self))
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    #[inline(always)]
    pub(crate) fn subscribe(&self, task: Task) {
        unsafe { self.wait_queue.push(task) };
    }

    #[inline(always)]
    pub unsafe fn unlock(&self) {
        // fast path
        if likely(self.counter.compare_exchange(self.expected_count.get(), 0, Release, Relaxed).is_ok()) {
            self.expected_count.set(1);
            return;
        }

        self.expected_count.set(self.expected_count.get() + 1);
        let next = self.wait_queue.pop();
        if likely(next.is_some()) {
            unsafe { local_executor_unchecked().exec_task(next.unwrap_unchecked()) };
        } else { // Another task failed to acquire a lock, but it is not yet in the queue
            let backoff = Backoff::new();
            loop {
                backoff.spin();
                let next = self.wait_queue.pop();
                if next.is_some() {
                    unsafe { local_executor_unchecked().exec_task(next.unwrap_unchecked()) };
                    break;
                }
            }
        }
    }
}

unsafe impl<T: Send> Sync for Mutex<T> {}
unsafe impl<T: Send> Send for Mutex<T> {}

pub mod naive {
    use std::cell::UnsafeCell;
    use std::mem::ManuallyDrop;
    use std::ops::{Deref, DerefMut};
    use std::ptr;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
    use crossbeam::utils::{Backoff, CachePadded};
    use crate::yield_now;

    pub struct MutexGuard<'mutex, T> {
        mutex: &'mutex Mutex<T>
    }

    impl<'mutex, T> MutexGuard<'mutex, T> {
        #[inline(always)]
        pub(crate) fn new(mutex: &'mutex Mutex<T>) -> Self {
            Self { mutex }
        }

        #[inline(always)]
        pub fn mutex(&self) -> &Mutex<T> {
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
        pub(crate) fn into_local_mutex(self) -> Mutex<T> {
            unsafe {
                self.mutex.unlock();
                ptr::read(ManuallyDrop::new(self).mutex)
            }
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

    pub struct Mutex<T> {
        is_locked: CachePadded<AtomicBool>,
        value: UnsafeCell<T>,
    }

    impl<T> Mutex<T> {
        #[inline(always)]
        pub fn new(value: T) -> Mutex<T> {
            Mutex {
                is_locked: CachePadded::new(AtomicBool::new(false)),
                value: UnsafeCell::new(value),
            }
        }

        #[inline(always)]
        pub async fn lock(&self) -> MutexGuard<T> {
            loop {
                let backoff = Backoff::new();
                while !backoff.is_completed() {
                    if let Some(guard) = self.try_lock() {
                        return guard;
                    }
                    backoff.spin();
                }

                yield_now().await;
            }
        }

        #[inline(always)]
        pub fn try_lock(&self) -> Option<MutexGuard<T>> {
            if self.is_locked.compare_exchange(false, true, Acquire, Relaxed).is_ok() {
                Some(MutexGuard::new(self))
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
            self.is_locked.store(false, Release);
        }
    }

    unsafe impl<T: Send> Sync for Mutex<T> {}
    unsafe impl<T: Send> Send for Mutex<T> {}
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;
    use crate::{end_local_thread, Executor};
    use crate::sleep::sleep;
    use crate::sync::WaitGroup;

    use super::*;

    #[test_macro::test]
    fn test_mutex() {
        const SLEEP_DURATION: Duration = Duration::from_millis(1);

        let mutex = Arc::new(Mutex::new(false));
        let wg = Arc::new(WaitGroup::new());

        let mutex_clone = mutex.clone();
        let wg_clone = wg.clone();
        wg_clone.add(1);
        thread::spawn(move || {
            let ex = Executor::init();
            ex.spawn_local(async move {
                let mut value = mutex_clone.lock().await;
                wg_clone.done();
                println!("1");
                sleep(SLEEP_DURATION).await;
                println!("3");
                *value = true;

                end_local_thread();
            });
            ex.run();
        });

        let _ = wg.wait().await;
        println!("2");
        let value = mutex.lock().await;
        println!("4");

        assert_eq!(*value, true);
        drop(value);
    }

    #[test_macro::test]
    fn test_try_mutex() {
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
        thread::spawn(move || {
            let ex = Executor::init();
            ex.spawn_local(async move {
                let mut value = mutex_clone.lock().await;
                println!("1");
                lock_wg_clone.done();
                let _ = unlock_wg_clone.wait().await;
                println!("4");
                *value = true;
                drop(value);
                second_lock_clone.done();

                end_local_thread();
            });
            ex.run();
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

    #[test_macro::test]
    // TODO
    fn stress_test_mutex() {
        const SLEEP_DURATION: Duration = Duration::from_micros(1);
        const PAR: usize = 10;
        const TRIES: usize = 200;

        // TODO not naive
        async fn work_with_lock(mutex: &naive::Mutex<usize>, wg: &WaitGroup) {
            let mut lock = mutex.lock().await;
            *lock += 1;
            if *lock % 100 == 0 {
                sleep(SLEEP_DURATION).await;
            }
            if *lock % 500 == 0 {
                println!("{} of {}", *lock, TRIES * PAR);
            }

            wg.done();
        }

        let mutex = Arc::new(naive::Mutex::new(0));
        let wg = Arc::new(WaitGroup::new());
        wg.add(PAR * TRIES);
        for _ in 1..PAR {
            let wg = wg.clone();
            let mutex = mutex.clone();
            thread::spawn(move || {
                let ex = Executor::init();
                ex.spawn_local(async move {
                    for _ in 0..TRIES {
                        work_with_lock(&mutex, &wg).await;
                    }

                    end_local_thread();
                });
                ex.run();
            });
        }

        for _ in 0..TRIES {
            work_with_lock(&mutex, &wg).await;
        }

        let _ = wg.wait().await;
    }

    // TODO test mod naive
}
