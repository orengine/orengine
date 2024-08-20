use std::cell::{Cell, UnsafeCell};
use std::future::Future;
use std::intrinsics::{likely, unlikely};
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize};
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::task::{Context, Poll};
use crossbeam::queue::SegQueue;
use crossbeam::utils::{Backoff, CachePadded};
use crate::Executor;
use crate::runtime::task::Task;

pub struct MutexGuard<'mutex, T> {
    mutex: &'mutex Mutex<T>
}

impl<'mutex, T> MutexGuard<'mutex, T> {
    #[inline(always)]
    pub(crate) fn new(mutex: &'mutex Mutex<T>) -> Self {
        Self { mutex }
    }

    #[inline(always)]
    pub fn local_mutex(&self) -> &Mutex<T> {
        &self.mutex
    }

    #[inline(always)]
    pub(crate) fn into_local_mutex(self) -> Mutex<T> {
        self.mutex.drop_lock();
        unsafe { (ManuallyDrop::new(self).mutex as *const Mutex<T>).read() }
    }
}

impl<'guard, T> Deref for MutexGuard<'guard, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mutex.value.get() }
    }
}

impl<'guard, T> DerefMut for MutexGuard<'guard, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.value.get() }
    }
}

impl<'guard, T> Drop for MutexGuard<'guard, T> {
    fn drop(&mut self) {
        self.mutex.drop_lock();
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

            let task = unsafe { (_cx.waker().as_raw().data() as *const Task).read() };
            this.mutex.wait_queue.push(task);
            this.was_called = true;
            Poll::Pending
        } else {
            Poll::Ready(MutexGuard::new(&this.mutex))
        }
    }
}

pub struct Mutex<T> {
    counter: CachePadded<AtomicUsize>,
    wait_queue: SegQueue<Task>,
    value: UnsafeCell<T>,
    expected_count: Cell<usize>
}

impl<T> Mutex<T> {
    #[inline(always)]
    pub fn new(value: T) -> Mutex<T> {
        Mutex {
            counter: CachePadded::new(AtomicUsize::new(0)),
            wait_queue: SegQueue::new(),
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
        self.wait_queue.push(task);
    }

    #[inline(always)]
    fn drop_lock(&self) {
        let next = self.wait_queue.pop();
        if unlikely(next.is_some()) {
            self.expected_count.set(self.expected_count.get() + 1);
            Executor::exec_task(unsafe { next.unwrap_unchecked() });
        } else {
            if unlikely(
                self.counter.compare_exchange(self.expected_count.get(), 0, Release, Relaxed).is_err()
            ) { // Another task failed to acquire a lock, but it is not yet in the queue
                let backoff = Backoff::new();
                loop {
                    backoff.spin();
                    let next = self.wait_queue.pop();
                    if next.is_some() {
                        self.expected_count.set(self.expected_count.get() + 1);
                        Executor::exec_task(unsafe { next.unwrap_unchecked() });
                        break;
                    }
                }
            } else {
                self.expected_count.set(1);
            }
        }
    }
}

unsafe impl<T: Send> Sync for Mutex<T> {}
unsafe impl<T: Send> Send for Mutex<T> {}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};
    use crate::runtime::create_local_executer_for_block_on;
    use crate::sleep::sleep;
    use super::*;

    #[test]
    fn test_mutex() {
        const SLEEP_DURATION: Duration = Duration::from_millis(1);

        let mutex = Arc::new(Mutex::new(false));
        let mutex_clone = mutex.clone();
        let (was_started, cond_var) = (
            Arc::new(std::sync::Mutex::new(false)),
            Arc::new(std::sync::Condvar::new())
        );
        let (was_started_clone, cond_var_clone) = (was_started.clone(), cond_var.clone());

        thread::spawn(move || {
           create_local_executer_for_block_on(async move {
               *was_started_clone.lock().unwrap() = true;
               cond_var_clone.notify_one();

               let mut value = mutex_clone.lock().await;
               println!("1");
               sleep(SLEEP_DURATION).await;
               println!("3");
               *value = true;
           });
        });

        create_local_executer_for_block_on(async move {
            let mut was_started = was_started.lock().unwrap();
            while !*was_started {
                was_started = cond_var.wait(was_started).unwrap();
            }

            let start = Instant::now();
            println!("2");
            let value = mutex.lock().await;
            println!("4");

            let elapsed = start.elapsed();
            assert!(elapsed >= SLEEP_DURATION);
            assert_eq!(*value, true);
            drop(value);
        });
    }

    #[test]
    fn test_try_mutex() {
        const SLEEP_DURATION: Duration = Duration::from_millis(1);

        create_local_executer_for_block_on(async move {
            let start = Instant::now();
            let mutex = Arc::new(Mutex::new(false));
            let mutex_clone = mutex.clone();
            Executor::exec_future(async move {
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
        });
    }
}