use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::panic_if_local_in_future;
use crate::runtime::{Task, local_executor};
use crate::sync::{Mutex, MutexGuard};
use crate::sync_task_queue::SyncTaskList;

/// Current state of the [`WaitCondVar`].
enum State {
    WaitSleep,
    WaitWake,
    WaitLock,
}

/// `WaitCondVar` represents a future returned by the [`CondVar::wait`] method.
///
/// It is used to wait for a notification from a condition variable.
pub struct WaitCondVar<'mutex, 'cond_var, T> {
    state: State,
    cond_var: &'cond_var CondVar,
    mutex: &'mutex Mutex<T>,
}

impl<'mutex, 'cond_var, T> WaitCondVar<'mutex, 'cond_var, T> {
    /// Creates a new [`WaitCondVar`].
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

/// `CondVar` is a condition variable that allows tasks to wait until
/// notified by another task.
///
/// It is designed to be used in conjunction with a [`Mutex`] to provide a way for tasks
/// to wait for a specific condition to occur.
///
/// # Attention
///
/// Drop a lock before call [`notify_one`](LocalCondVar::notify_one)
/// or [`notify_all`](LocalCondVar::notify_all) to improve performance.
///
/// # The difference between `CondVar` and [`LocalCondVar`](crate::sync::LocalCondVar)
///
/// The `CondVar` works with `global tasks` and can be shared between threads.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// # Example
///
/// ```no_run
/// use orengine::sync::{CondVar, Mutex, global_scope};
/// use orengine::sleep;
/// use std::time::Duration;
///
/// # async fn test() {
/// let cvar = CondVar::new();
/// let is_ready = Mutex::new(false);
///
/// global_scope(|scope| async {
///     scope.spawn(async {
///         sleep(Duration::from_secs(1)).await;
///         let mut lock = is_ready.lock().await;
///         *lock = true;
///         lock.unlock();
///         cvar.notify_one();
///     });
///
///     let mut lock = is_ready.lock().await;
///     while !*lock {
///         lock = cvar.wait(lock).await; // wait 1 second
///     }
/// }).await;
/// # }
/// ```
pub struct CondVar {
    wait_queue: SyncTaskList,
}

impl CondVar {
    /// Creates a new [`CondVar`].
    #[inline(always)]
    pub fn new() -> CondVar {
        CondVar {
            wait_queue: SyncTaskList::new(),
        }
    }

    /// Wait a notification.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::sync::{CondVar, Mutex, global_scope};
    /// use orengine::sleep;
    /// use std::time::Duration;
    ///
    /// # async fn test() {
    /// let cvar = CondVar::new();
    /// let is_ready = Mutex::new(false);
    ///
    /// global_scope(|scope| async {
    ///     scope.spawn(async {
    ///         sleep(Duration::from_secs(1)).await;
    ///         let mut lock = is_ready.lock().await;
    ///         *lock = true;
    ///         lock.unlock();
    ///         cvar.notify_one();
    ///     });
    ///
    ///     let mut lock = is_ready.lock().await;
    ///     while !*lock {
    ///         lock = cvar.wait(lock).await; // wait 1 second
    ///     }
    /// }).await;
    /// # }
    #[inline(always)]
    pub fn wait<'mutex, 'cond_var, T>(
        &'cond_var self,
        mutex_guard: MutexGuard<'mutex, T>,
    ) -> WaitCondVar<'mutex, 'cond_var, T> {
        WaitCondVar::new(self, mutex_guard.into_mutex())
    }

    /// Notifies one waiting task.
    ///
    /// # Attention
    ///
    /// Drop a lock before call [`notify_one`](CondVar::notify_one).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::sync::{Mutex, CondVar};
    ///
    /// async fn inc_counter_and_notify(counter: &Mutex<i32>, cvar: &CondVar) {
    ///     let mut lock = counter.lock().await;
    ///     *lock += 1;
    ///     lock.unlock();
    ///     cvar.notify_one();
    /// }
    /// ```
    #[inline(always)]
    pub fn notify_one(&self) {
        if let Some(task) = self.wait_queue.pop() {
            local_executor().spawn_global_task(task)
        }
    }

    /// Notifies all waiting tasks.
    ///
    /// # Attention
    ///
    /// Drop a lock before call [`notify_one`](CondVar::notify_one).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::sync::{Mutex, CondVar};
    ///
    /// async fn inc_counter_and_notify_all(counter: &Mutex<i32>, cvar: &CondVar) {
    ///     let mut lock = counter.lock().await;
    ///     *lock += 1;
    ///     lock.unlock();
    ///     cvar.notify_all();
    /// }
    /// ```
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

    use crate::Executor;
    use crate::runtime::local_executor;
    use crate::sleep::sleep;
    use crate::sync::WaitGroup;

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
