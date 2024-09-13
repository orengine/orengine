use std::cell::UnsafeCell;
use std::future::Future;
use std::intrinsics::unlikely;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::task::{Context, Poll};
use crate::runtime::local_executor;
use crate::runtime::task::Task;

pub struct LocalMutexGuard<'mutex, T> {
    local_mutex: &'mutex LocalMutex<T>
}

impl<'mutex, T> LocalMutexGuard<'mutex, T> {
    #[inline(always)]
    pub(crate) fn new(local_mutex: &'mutex LocalMutex<T>) -> Self {
        Self { local_mutex }
    }

    #[inline(always)]
    pub fn local_mutex(&'mutex self) -> &'mutex LocalMutex<T> {
        &self.local_mutex
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
    pub fn into_local_mutex(self) -> &'mutex LocalMutex<T> {
        &self.local_mutex
    }

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

pub struct MutexWait<'mutex, T> {
    need_wait: bool,
    local_mutex: &'mutex LocalMutex<T>
}

impl<'mutex, T> MutexWait<'mutex, T> {
    #[inline(always)]
    fn new(need_wait: bool, local_mutex: &'mutex LocalMutex<T>) -> Self {
        Self {
            need_wait,
            local_mutex
        }
    }
}

impl<'mutex, T> Future for MutexWait<'mutex, T> {
    type Output = LocalMutexGuard<'mutex, T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        if unlikely(this.need_wait) {
            let task = unsafe { (cx.waker().data() as *const Task).read() };
            let wait_queue = unsafe { &mut *this.local_mutex.wait_queue.get() };
            wait_queue.push(task);
            this.need_wait = false;
            Poll::Pending
        } else {
            Poll::Ready(LocalMutexGuard::new(this.local_mutex))
        }
    }
}

pub struct LocalMutex<T> {
    is_locked: UnsafeCell<bool>,
    wait_queue: UnsafeCell<Vec<Task>>,
    value: UnsafeCell<T>
}

impl<T> LocalMutex<T> {
    #[inline(always)]
    pub fn new(value: T) -> LocalMutex<T> {
        LocalMutex {
            is_locked: UnsafeCell::new(false),
            wait_queue: UnsafeCell::new(Vec::new()),
            value: UnsafeCell::new(value)
        }
    }

    #[inline(always)]
    pub fn lock(&self) -> MutexWait<T> {
        let is_locked = unsafe { &mut *self.is_locked.get() };
        if !*is_locked {
            *is_locked = true;
            MutexWait::new(false, self)
        } else {
            MutexWait::new(true, self)
        }
    }

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

    #[inline(always)]
    pub(crate) fn subscribe(&self, task: Task) {
        let wait_queue = unsafe { &mut *self.wait_queue.get() };
        wait_queue.push(task);
    }

    #[inline(always)]
    pub unsafe fn unlock(&self) {
        debug_assert!(self.is_locked.get().read());

        let wait_queue = unsafe { &mut *self.wait_queue.get() };
        let next = wait_queue.pop();
        if unlikely(next.is_some()) {
            local_executor().exec_task(unsafe { next.unwrap_unchecked() });
        } else {
            let is_locked = unsafe { &mut *self.is_locked.get() };
            *is_locked = false;
        }
    }

    #[inline(always)]
    pub unsafe fn get_locked(&self) -> &mut T {
        debug_assert!(
            self.is_locked.get().read(),
            "LocalMutex is unlocked, but calling get_locked it must be locked"
        );

        &mut *self.value.get()
    }
}

unsafe impl<T> Sync for LocalMutex<T> {}
impl<T> !Send for LocalMutex<T> {}

#[cfg(test)]
mod tests {
    use std::rc::Rc;
    use std::time::{Duration, Instant};
    use crate::sleep::sleep;
    use super::*;

    #[orengine_macros::test]
    fn test_mutex() {
        let start = Instant::now();
        const SLEEP_DURATION: Duration = Duration::from_millis(1);

        let mutex = Rc::new(LocalMutex::new(false));
        let mutex_clone = mutex.clone();
        local_executor().exec_task(Task::from_future(async move {
            let mut value = mutex_clone.lock().await;
            println!("1");
            sleep(SLEEP_DURATION).await;
            println!("3");
            *value = true;
        }));

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
        local_executor().exec_task(Task::from_future(async move {
            let mut value = mutex_clone.lock().await;
            println!("1");
            sleep(SLEEP_DURATION).await;
            println!("4");
            *value = true;
        }));

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