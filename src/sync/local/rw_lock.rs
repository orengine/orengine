use std::cell::UnsafeCell;
use std::future::Future;
use std::intrinsics::{likely, unlikely};
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::task::{Context, Poll};
use crate::runtime::local_executor;
use crate::runtime::task::Task;

pub struct LocalReadLockGuard<'rw_lock, T> {
    local_rw_lock: &'rw_lock LocalRWLock<T>
}

impl<'rw_lock, T> LocalReadLockGuard<'rw_lock, T> {
    #[inline(always)]
    fn new(local_rw_lock: &'rw_lock LocalRWLock<T>) -> Self {
        Self { local_rw_lock }
    }

    #[inline(always)]
    pub fn local_rw_lock(&self) -> &LocalRWLock<T> {
        &self.local_rw_lock
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
}

impl<'rw_lock, T> Deref for LocalReadLockGuard<'rw_lock, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.local_rw_lock.get_inner().value
    }
}

impl<'rw_lock, T> Drop for LocalReadLockGuard<'rw_lock, T> {
    fn drop(&mut self) {
        unsafe { self.local_rw_lock.read_unlock(); }
    }
}

pub struct LocalWriteLockGuard<'rw_lock, T> {
    local_rw_lock: &'rw_lock LocalRWLock<T>
}

impl<'rw_lock, T> LocalWriteLockGuard<'rw_lock, T> {
    #[inline(always)]
    fn new(local_rw_lock: &'rw_lock LocalRWLock<T>) -> Self {
        Self { local_rw_lock }
    }

    #[inline(always)]
    pub fn local_rw_lock(&self) -> &LocalRWLock<T> {
        &self.local_rw_lock
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
        unsafe { self.local_rw_lock.write_unlock(); }
    }
}

pub struct ReadLockWait<'rw_lock, T> {
    need_wait: bool,
    local_rw_lock: &'rw_lock LocalRWLock<T>
}

impl<'rw_lock, T> ReadLockWait<'rw_lock, T> {
    #[inline(always)]
    fn new(need_wait: bool, local_rw_lock: &'rw_lock LocalRWLock<T>) -> Self {
        Self {
            need_wait,
            local_rw_lock
        }
    }
}

impl<'rw_lock, T> Future for ReadLockWait<'rw_lock, T> {
    type Output = LocalReadLockGuard<'rw_lock, T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        if unlikely(this.need_wait) {
            let task = unsafe { (cx.waker().as_raw().data() as *const Task).read() };
            this.local_rw_lock.get_inner().wait_queue_read.push(task);
            this.need_wait = false;
            Poll::Pending
        } else {
            Poll::Ready(LocalReadLockGuard::new(this.local_rw_lock))
        }
    }
}

pub struct WriteLockWait<'rw_lock, T> {
    need_wait: bool,
    local_rw_lock: &'rw_lock LocalRWLock<T>
}

impl<'rw_lock, T> WriteLockWait<'rw_lock, T> {
    #[inline(always)]
    fn new(need_wait: bool, local_rw_lock: &'rw_lock LocalRWLock<T>) -> Self {
        Self {
            need_wait,
            local_rw_lock
        }
    }
}

impl<'rw_lock, T> Future for WriteLockWait<'rw_lock, T> {
    type Output = LocalWriteLockGuard<'rw_lock, T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        if unlikely(this.need_wait) {
            let task = unsafe { (cx.waker().as_raw().data() as *const Task).read() };
            this.local_rw_lock.get_inner().wait_queue_write.push(task);
            this.need_wait = false;
            Poll::Pending
        } else {
            Poll::Ready(LocalWriteLockGuard::new(this.local_rw_lock))
        }
    }
}

struct Inner<T> {
    wait_queue_read: Vec<Task>,
    wait_queue_write: Vec<Task>,
    number_of_readers: isize,
    value: T
}

pub struct LocalRWLock<T> {
    inner: UnsafeCell<Inner<T>>
}

impl<T> LocalRWLock<T> {
    #[inline(always)]
    pub fn new(value: T) -> LocalRWLock<T> {
        LocalRWLock {
            inner: UnsafeCell::new(Inner {
                wait_queue_read: Vec::new(),
                wait_queue_write: Vec::new(),
                number_of_readers: 0,
                value
            })
        }
    }

    #[inline(always)]
    fn get_inner(&self) -> &mut Inner<T> {
        unsafe { &mut *self.inner.get() }
    }

    #[inline(always)]
    pub fn write(&self) -> WriteLockWait<T> {
        let inner = self.get_inner();

        if unlikely(inner.number_of_readers == 0) {
            debug_assert!(inner.wait_queue_read.is_empty());

            inner.number_of_readers = -1;
            return WriteLockWait::new(false, self);
        }

        WriteLockWait::new(true, self)
    }

    #[inline(always)]
    pub fn read(&self) -> ReadLockWait<T> {
        let inner = self.get_inner();

        if likely(inner.number_of_readers > -1) {
            inner.number_of_readers += 1;
            ReadLockWait::new(false, self)
        } else {
            ReadLockWait::new(true, self)
        }
    }

    #[inline(always)]
    pub fn try_write(&self) -> Option<LocalWriteLockGuard<T>> {
        let inner = self.get_inner();

        if unlikely(inner.number_of_readers == 0) {
            debug_assert!(inner.wait_queue_read.is_empty());

            inner.number_of_readers = -1;
            Some(LocalWriteLockGuard::new(self))
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn try_read(&self) -> Option<LocalReadLockGuard<T>> {
        let inner = self.get_inner();
        if likely(inner.number_of_readers > -1) {
            inner.number_of_readers += 1;
            Some(LocalReadLockGuard::new(self))
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner.get_mut().value
    }

    #[inline(always)]
    pub unsafe fn read_unlock(&self) {
        let inner = self.get_inner();
        inner.number_of_readers -= 1;

        if inner.number_of_readers == 0 {
            debug_assert!(inner.wait_queue_read.is_empty());
            let task = inner.wait_queue_write.pop();
            if unlikely(task.is_some()) {
                inner.number_of_readers = -1;
                local_executor().exec_task(unsafe { task.unwrap_unchecked() });
            }
        }
    }

    #[inline(always)]
    pub unsafe fn write_unlock(&self) {
        let inner = self.get_inner();

        let task = inner.wait_queue_write.pop();
        if unlikely(task.is_some()) {
            local_executor().exec_task(task.unwrap());
        } else {
            let mut readers_count = inner.wait_queue_read.len();
            inner.number_of_readers = readers_count as isize;
            while readers_count > 0 {
                let task = inner.wait_queue_read.pop();
                local_executor().exec_task(unsafe { task.unwrap_unchecked() });
                readers_count -= 1;
            }
        }
    }
}

unsafe impl<T> Sync for LocalRWLock<T> {}
impl<T> !Send for LocalRWLock<T> {}

#[cfg(test)]
mod tests {
    use std::rc::Rc;
    use std::time::{Duration, Instant};
    use crate::sleep::sleep;
    use crate::sync::LocalWaitGroup;
    use super::*;

    #[test_macro::test]
    fn test_rw_lock() {
        const SLEEP_DURATION: Duration = Duration::from_millis(1);

        let start = Instant::now();
        let mutex = Rc::new(LocalRWLock::new(0));
        let wg = Rc::new(LocalWaitGroup::new());
        let read_wg = Rc::new(LocalWaitGroup::new());

        for i in 1..=30 {
            let mutex = mutex.clone();
            local_executor().exec_future(async move {
                let value = mutex.read().await;
                assert_eq!(mutex.get_inner().number_of_readers, i);
                assert_eq!(*value, 0);
                sleep(SLEEP_DURATION).await;
                assert_eq!(mutex.get_inner().number_of_readers, 31 - i);
                assert_eq!(*value, 0);
            });
        }

        for _ in 1..=30 {
            let wg = wg.clone();
            let read_wg = read_wg.clone();
            wg.add(1);
            let mutex = mutex.clone();
            local_executor().exec_future(async move {
                assert_eq!(mutex.get_inner().number_of_readers, 30);
                let mut value = mutex.write().await;
                {
                    let read_wg = read_wg.clone();
                    let mutex = mutex.clone();
                    read_wg.add(1);

                    local_executor().exec_future(async move {
                        assert_eq!(mutex.get_inner().number_of_readers, -1);
                        let value = mutex.read().await;
                        assert_ne!(*value, 0);
                        assert_ne!(mutex.get_inner().number_of_readers, 0);
                        read_wg.done();
                    });
                }
                let elapsed = start.elapsed();
                assert!(elapsed >= SLEEP_DURATION);
                assert_eq!(mutex.get_inner().number_of_readers, -1);
                *value += 1;

                wg.done();
            });
        }

        wg.wait().await;
        read_wg.wait().await;

        let value = mutex.read().await;
        assert_eq!(*value, 30);
        assert_ne!(mutex.get_inner().number_of_readers, 0);
    }

    #[test_macro::test]
    fn test_try_rw_lock() {
        const SLEEP_DURATION: Duration = Duration::from_millis(1);

        let start = Instant::now();
        let mutex = Rc::new(LocalRWLock::new(0));
        let wg = Rc::new(LocalWaitGroup::new());
        let read_wg = Rc::new(LocalWaitGroup::new());

        for i in 1..=100 {
            let mutex = mutex.clone();
            local_executor().exec_future(async move {
                let value = mutex.try_read().expect("try_read failed");
                assert_eq!(mutex.get_inner().number_of_readers, i);
                assert_eq!(*value, 0);
                sleep(SLEEP_DURATION).await;
            });
        }

        for _i in 1..=100 {
            let wg = wg.clone();
            let read_wg = read_wg.clone();
            wg.add(1);
            let mutex = mutex.clone();
            local_executor().exec_future(async move {
                assert_eq!(mutex.get_inner().number_of_readers, 100);
                assert!(mutex.try_write().is_none());
                sleep(2 * SLEEP_DURATION).await;
                let mut value = mutex.try_write().expect("try_write failed");
                read_wg.add(1);
                {
                    let mutex = mutex.clone();

                    local_executor().exec_future(async move {
                        assert_eq!(mutex.get_inner().number_of_readers, -1);
                        assert!(mutex.try_read().is_none());
                        sleep(SLEEP_DURATION * 2).await;
                        let value = mutex.try_read().expect("try_read failed");
                        assert_ne!(*value, 0);
                        assert_ne!(mutex.get_inner().number_of_readers, 0);
                        read_wg.done();
                    });
                }
                let elapsed = start.elapsed();
                assert!(elapsed >= SLEEP_DURATION);
                assert_eq!(mutex.get_inner().number_of_readers, -1);
                *value += 1;

                wg.done();
            });
        }

        wg.wait().await;
        read_wg.wait().await;

        let value = mutex.try_read().expect("try_read failed");
        assert_eq!(*value, 100);
        assert_ne!(mutex.get_inner().number_of_readers, 0);
    }
}