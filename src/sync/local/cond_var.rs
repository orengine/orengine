use std::cell::UnsafeCell;
use std::future::Future;
use std::intrinsics::likely;
use std::pin::Pin;
use std::task::{Context, Poll};
use crate::Executor;
use crate::runtime::task::Task;
use crate::sync::{LocalMutex, LocalMutexGuard};

enum State {
    WaitSleep,
    WaitWake,
    WaitLock
}

pub struct WaitCondVar<'mutex, 'cond_var, T> {
    state: State,
    cond_var: &'cond_var LocalCondVar,
    local_mutex: &'mutex LocalMutex<T>
}

impl<'mutex, 'cond_var, T> WaitCondVar<'mutex, 'cond_var, T> {
    #[inline(always)]
    pub fn new(
        cond_var: &'cond_var LocalCondVar,
        local_mutex: &'mutex LocalMutex<T>
    ) -> WaitCondVar<'mutex, 'cond_var, T> {
        WaitCondVar {
            state: State::WaitSleep,
            cond_var,
            local_mutex
        }
    }
}

impl<'mutex, 'cond_var, T> Future for WaitCondVar<'mutex, 'cond_var, T> {
    type Output = LocalMutexGuard<'mutex, T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        match this.state {
            State::WaitSleep => {
                this.state = State::WaitWake;
                let task = unsafe { (cx.waker().as_raw().data() as *mut Task).read() };
                let wait_queue = unsafe { &mut *this.cond_var.wait_queue.get() };
                wait_queue.push(task);
                Poll::Pending
            },
            State::WaitWake => {
                match this.local_mutex.try_lock() {
                    Some(guard) => Poll::Ready(guard),
                    None => {
                        this.state = State::WaitLock;
                        let task = unsafe { (cx.waker().as_raw().data() as *mut Task).read() };
                        this.local_mutex.subscribe(task);
                        Poll::Pending
                    }
                }
            },
            State::WaitLock => {
                Poll::Ready(LocalMutexGuard::new(this.local_mutex))
            }
        }
    }
}

// TODO: in docs say to drop(guard) before notify
pub struct LocalCondVar {
    wait_queue: UnsafeCell<Vec<Task>>
}

impl LocalCondVar {
    #[inline(always)]
    pub fn new() -> LocalCondVar {
        LocalCondVar {
            wait_queue: UnsafeCell::new(Vec::new())
        }
    }

    #[inline(always)]
    pub fn wait<'mutex, 'cond_var, T>(
        &'cond_var self,
        local_mutex_guard: LocalMutexGuard<'mutex, T>
    ) -> WaitCondVar<'mutex, 'cond_var, T> {
        WaitCondVar::new(self, local_mutex_guard.into_local_mutex())
    }

    #[inline(always)]
    pub fn notify_one(&self) {
        let wait_queue = unsafe { &mut *self.wait_queue.get() };
        if likely(!wait_queue.is_empty()) {
            let task = wait_queue.pop();
            Executor::exec_task(unsafe { task.unwrap_unchecked() });
        }
    }

    #[inline(always)]
    pub fn notify_all(&self) {
        let wait_queue = unsafe { &mut *self.wait_queue.get() };
        while let Some(task) = wait_queue.pop() {
            Executor::exec_task(task);
        }
    }
}

unsafe impl Sync for LocalCondVar {}

#[cfg(test)]
mod tests {
    use std::ops::Deref;
    use std::rc::Rc;
    use std::time::{Duration, Instant};
    use crate::runtime::{create_local_executer_for_block_on, local_executor};
    use crate::sleep::sleep;
    use crate::sync::LocalWaitGroup;
    use super::*;

    const TIME_TO_SLEEP: Duration = Duration::from_millis(1);

    fn test_one(need_drop: bool) {
        create_local_executer_for_block_on(async move {
            let start = Instant::now();
            let pair = Rc::new((LocalMutex::new(false), LocalCondVar::new()));
            let pair2 = pair.clone();
            // Inside our lock, spawn a new thread, and then wait for it to start.
            local_executor().spawn_local(async move {
                let (lock, cvar) = pair2.deref();
                let mut started = lock.lock().await;
                sleep(TIME_TO_SLEEP).await;
                *started = true;
                if need_drop {
                    drop(started);
                }
                // We notify the condvar that the value has changed.
                cvar.notify_one();
            });

            // Wait for the thread to start up.
            let (lock, cvar) = pair.deref();
            let mut started = lock.lock().await;
            while !*started {
                started = cvar.wait(started).await;
            }

            assert!(start.elapsed() >= TIME_TO_SLEEP);
        });
    }

    fn test_all(need_drop: bool) {
        create_local_executer_for_block_on(async move {
            const NUMBER_OF_WAITERS: usize = 10;

            let start = Instant::now();
            let pair = Rc::new((LocalMutex::new(false), LocalCondVar::new()));
            let pair2 = pair.clone();
            // Inside our lock, spawn a new thread, and then wait for it to start.
            local_executor().spawn_local(async move {
                let (lock, cvar) = pair2.deref();
                let mut started = lock.lock().await;
                sleep(TIME_TO_SLEEP).await;
                *started = true;
                if need_drop {
                    drop(started);
                }
                // We notify the condvar that the value has changed.
                cvar.notify_all();
            });

            let wg = Rc::new(LocalWaitGroup::new());
            for _ in 0..NUMBER_OF_WAITERS {
                let pair = pair.clone();
                let wg = wg.clone();
                wg.add(1);
                local_executor().spawn_local(async move {
                    let (lock, cvar) = pair.deref();
                    let mut started = lock.lock().await;
                    while !*started {
                        started = cvar.wait(started).await;
                    }
                    wg.done();
                });
            }

            wg.wait().await;

            assert!(start.elapsed() >= TIME_TO_SLEEP);
        });
    }

    #[test]
    fn test_one_with_drop_guard() {
        test_one(true);
    }

    #[test]
    fn test_all_with_drop_guard() {
        test_all(true);
    }

    #[test]
    fn test_one_without_drop_guard() {
        test_one(false);
    }

    #[test]
    fn test_all_without_drop_guard() {
        test_all(false);
    }
}