use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::atomic_task_queue::AtomicTaskList;
use crate::panic_if_local_in_future;
use crate::runtime::{local_executor, Task};
use crate::sync::{Mutex, MutexGuard};

enum State {
    WaitSleep,
    WaitWake,
    WaitLock,
}

pub struct WaitCondVar<'mutex, 'cond_var, T> {
    state: State,
    cond_var: &'cond_var CondVar,
    mutex: &'mutex Mutex<T>,
}

impl<'mutex, 'cond_var, T> WaitCondVar<'mutex, 'cond_var, T> {
    #[inline(always)]
    pub fn new(
        cond_var: &'cond_var CondVar,
        mutex: &'mutex Mutex<T>,
    ) -> WaitCondVar<'mutex, 'cond_var, T> {
        WaitCondVar {
            state: State::WaitSleep,
            cond_var,
            mutex,
        }
    }
}

impl<'mutex, 'cond_var, T> Future for WaitCondVar<'mutex, 'cond_var, T> {
    type Output = MutexGuard<'mutex, T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        panic_if_local_in_future!(cx, "CondVar");

        match this.state {
            State::WaitSleep => {
                this.state = State::WaitWake;
                unsafe { local_executor().push_current_task_to(&this.cond_var.wait_queue) };
                Poll::Pending
            }
            State::WaitWake => match this.mutex.try_lock_with_spinning() {
                Some(guard) => Poll::Ready(guard),
                None => {
                    this.state = State::WaitLock;
                    let task = unsafe { (cx.waker().data() as *mut Task).read() };
                    unsafe {
                        this.mutex.subscribe(task);
                    }
                    Poll::Pending
                }
            },
            State::WaitLock => Poll::Ready(MutexGuard::new(this.mutex)),
        }
    }
}

// TODO: in docs say to drop(guard) before notify
pub struct CondVar {
    wait_queue: AtomicTaskList,
}

impl CondVar {
    #[inline(always)]
    pub fn new() -> CondVar {
        CondVar {
            wait_queue: AtomicTaskList::new(),
        }
    }

    #[inline(always)]
    pub fn wait<'mutex, 'cond_var, T>(
        &'cond_var self,
        mutex_guard: MutexGuard<'mutex, T>,
    ) -> WaitCondVar<'mutex, 'cond_var, T> {
        WaitCondVar::new(self, mutex_guard.into_mutex())
    }

    #[inline(always)]
    pub fn notify_one(&self) {
        if let Some(task) = self.wait_queue.pop() {
            local_executor().spawn_global_task(task)
        }
    }

    #[inline(always)]
    pub fn notify_all(&self) {
        let executor = local_executor();
        while let Some(task) = self.wait_queue.pop() {
            executor.spawn_global_task(task);
        }
    }
}

unsafe impl Sync for CondVar {}
unsafe impl Send for CondVar {}

#[cfg(test)]
mod tests {
    use std::ops::Deref;
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    use crate::runtime::local_executor;
    use crate::sleep::sleep;
    use crate::sync::WaitGroup;
    use crate::{Executor};

    use super::*;

    const TIME_TO_SLEEP: Duration = Duration::from_millis(1);

    async fn test_one(need_drop: bool) {
        let start = Instant::now();
        let pair = Arc::new((Mutex::new(false), CondVar::new()));
        let pair2 = pair.clone();
        thread::spawn(move || {
            let ex = Executor::init();
            ex.run_with_global_future(async move {
                let (lock, cvar) = pair2.deref();
                let mut started = lock.lock().await;
                sleep(TIME_TO_SLEEP).await;
                *started = true;
                if need_drop {
                    drop(started);
                }
                cvar.notify_one();
            });
        });

        let (lock, cvar) = pair.deref();
        let mut started = lock.lock().await;
        while !*started {
            started = cvar.wait(started).await;
        }

        assert!(start.elapsed() >= TIME_TO_SLEEP);
    }

    async fn test_all(need_drop: bool) {
        const NUMBER_OF_WAITERS: usize = 10;

        let start = Instant::now();
        let pair = Arc::new((Mutex::new(false), CondVar::new()));
        let pair2 = pair.clone();
        local_executor().spawn_global(async move {
            let (lock, cvar) = pair2.deref();
            let mut started = lock.lock().await;
            sleep(TIME_TO_SLEEP).await;
            *started = true;
            if need_drop {
                drop(started);
            }
            cvar.notify_all();
        });

        let wg = Arc::new(WaitGroup::new());
        for _ in 0..NUMBER_OF_WAITERS {
            let pair = pair.clone();
            let wg = wg.clone();
            wg.add(1);
            thread::spawn(move || {
                let executor = Executor::init();
                executor.spawn_global(async move {
                    let (lock, cvar) = pair.deref();
                    let mut started = lock.lock().await;
                    while !*started {
                        started = cvar.wait(started).await;
                    }
                    wg.done();
                });
                executor.run();
            });
        }

        let _ = wg.wait().await;

        assert!(start.elapsed() >= TIME_TO_SLEEP);
    }

    #[orengine_macros::test_global]
    fn test_one_with_drop_guard() {
        test_one(true).await;
    }

    #[orengine_macros::test_global]
    fn test_all_with_drop_guard() {
        test_all(true).await;
    }

    #[orengine_macros::test_global]
    fn test_one_without_drop_guard() {
        test_one(false).await;
    }

    #[orengine_macros::test_global]
    fn test_all_without_drop_guard() {
        test_all(false).await;
    }
}
