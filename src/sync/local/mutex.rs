use std::future::Future;
use std::intrinsics::unlikely;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::task::{Context, Poll};
use crate::Executor;
use crate::local::Local;
use crate::runtime::task::Task;

pub struct LocalMutexGuard<T> {
    local_mutex: LocalMutex<T>
}

impl<T> LocalMutexGuard<T> {
    #[inline(always)]
    pub(crate) fn new(local_mutex: LocalMutex<T>) -> Self {
        Self { local_mutex }
    }

    #[inline(always)]
    pub fn local_mutex(&self) -> &LocalMutex<T> {
        &self.local_mutex
    }

    #[inline(always)]
    pub(crate) fn into_local_mutex(self) -> LocalMutex<T> {
        self.local_mutex.drop_lock();
        unsafe { (&ManuallyDrop::new(self).local_mutex as *const LocalMutex<T>).read() }
    }
}

impl<T> Deref for LocalMutexGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.local_mutex.inner.get().value
    }
}

impl<T> DerefMut for LocalMutexGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.local_mutex.inner.get_mut().value
    }
}

impl<T> Drop for LocalMutexGuard<T> {
    fn drop(&mut self) {
        self.local_mutex.drop_lock();
    }
}

pub struct MutexWait<T> {
    need_wait: bool,
    local_mutex: LocalMutex<T>,
    phantom_data: PhantomData<T>
}

impl<T> MutexWait<T> {
    #[inline(always)]
    fn new(need_wait: bool, local_mutex: LocalMutex<T>) -> Self {
        Self {
            need_wait,
            local_mutex,
            phantom_data: PhantomData
        }
    }
}

impl<T> Future for MutexWait<T> {
    type Output = LocalMutexGuard<T>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        if unlikely(this.need_wait) {
            let task = unsafe { (_cx.waker().as_raw().data() as *const Task).read() };
            this.local_mutex.inner.get_mut().wait_queue.push(task);
            this.need_wait = false;
            Poll::Pending
        } else {
            Poll::Ready(LocalMutexGuard::new(this.local_mutex.clone()))
        }
    }
}

struct Inner<T> {
    is_locked: bool,
    wait_queue: Vec<Task>,
    value: T
}

pub struct LocalMutex<T> {
    inner: Local<Inner<T>>
}

impl<T> LocalMutex<T> {
    #[inline(always)]
    pub fn new(value: T) -> LocalMutex<T> {
        LocalMutex {
            inner: Local::new(Inner {
                is_locked: false,
                wait_queue: Vec::new(),
                value
            })
        }
    }

    #[inline(always)]
    pub fn lock(&self) -> MutexWait<T> {
        let inner = self.inner.get_mut();

        if !inner.is_locked {
            inner.is_locked = true;
            MutexWait::new(false, self.clone())
        } else {
            MutexWait::new(true, self.clone())
        }
    }

    #[inline(always)]
    pub fn try_lock(&self) -> Option<LocalMutexGuard<T>> {
        let inner = self.inner.get_mut();

        if !inner.is_locked {
            inner.is_locked = true;
            Some(LocalMutexGuard::new(self.clone()))
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner.get_mut().value
    }

    #[inline(always)]
    pub(crate) fn subscribe(&self, task: Task) {
        let inner = self.inner.get_mut();
        inner.wait_queue.push(task);
    }

    #[inline(always)]
    fn drop_lock(&self) {
        let inner = self.inner.get_mut();
        let next = inner.wait_queue.pop();
        if unlikely(next.is_some()) {
            Executor::exec_task(unsafe { next.unwrap_unchecked() });
        } else {
            inner.is_locked = false;
        }
    }
}

impl<T> Clone for LocalMutex<T> {
    fn clone(&self) -> Self {
        LocalMutex {
            inner: self.inner.clone()
        }
    }
}

unsafe impl<T> Sync for LocalMutex<T> {}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};
    use crate::runtime::create_local_executer_for_block_on;
    use crate::sleep::sleep;
    use super::*;

    #[test]
    fn test_mutex() {
        let start = Instant::now();
        const SLEEP_DURATION: Duration = Duration::from_millis(1);

        create_local_executer_for_block_on(async move {
            let mutex = LocalMutex::new(false);
            let mutex_clone = mutex.clone();
            Executor::exec_task(Task::from_future(async move {
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
        });
    }

    #[test]
    fn test_try_mutex() {
        const SLEEP_DURATION: Duration = Duration::from_millis(1);

        create_local_executer_for_block_on(async move {
            let start = Instant::now();
            let mutex = LocalMutex::new(false);
            let mutex_clone = mutex.clone();
            Executor::exec_task(Task::from_future(async move {
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
        });
    }
}