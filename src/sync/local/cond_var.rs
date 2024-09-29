use crate::runtime::local_executor;
use crate::runtime::task::Task;
use crate::sync::{LocalMutex, LocalMutexGuard};
use std::cell::UnsafeCell;
use std::future::Future;
use std::intrinsics::likely;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Current state of the [`WaitCondVar`].
enum State {
    /// Default state.
    WaitSleep,
    /// The [`WaitCondVar`] is parked and will be woken up when [`LocalCondVar::notify`] is called.
    WaitWake,
    /// The [`WaitCondVar`] has been woken up, and it is parked on a [`LocalMutex`],
    /// because the [`LocalMutex`] is locked.
    WaitLock,
}

/// `WaitCondVar` represents a future returned by the [`LocalCondVar::wait`] method.
///
/// It is used to wait for a notification from a condition variable.
pub struct WaitCondVar<'mutex, 'cond_var, T> {
    state: State,
    cond_var: &'cond_var LocalCondVar,
    local_mutex: &'mutex LocalMutex<T>,
}

impl<'mutex, 'cond_var, T> WaitCondVar<'mutex, 'cond_var, T> {
    /// Creates a new [`WaitCondVar`].
    #[inline(always)]
    pub fn new(
        cond_var: &'cond_var LocalCondVar,
        local_mutex: &'mutex LocalMutex<T>,
    ) -> WaitCondVar<'mutex, 'cond_var, T> {
        WaitCondVar {
            state: State::WaitSleep,
            cond_var,
            local_mutex,
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
                let task = unsafe { (cx.waker().data() as *mut Task).read() };
                let wait_queue = unsafe { &mut *this.cond_var.wait_queue.get() };
                wait_queue.push(task);
                Poll::Pending
            }
            State::WaitWake => match this.local_mutex.try_lock() {
                Some(guard) => Poll::Ready(guard),
                None => {
                    this.state = State::WaitLock;
                    let task = unsafe { (cx.waker().data() as *mut Task).read() };
                    this.local_mutex.subscribe(task);
                    Poll::Pending
                }
            },
            State::WaitLock => Poll::Ready(LocalMutexGuard::new(this.local_mutex)),
        }
    }
}

/// LocalCondVar is a condition variable that allows tasks to wait until
/// notified by another task.
///
/// It is designed to be used in conjunction with a [`LocalMutex`] to provide a way for tasks
/// to wait for a specific condition to occur.
///
/// # Attention
///
/// Drop a lock before call [`notify_one`](LocalCondVar::notify_one)
/// or [`notify_all`](LocalCondVar::notify_all) to improve performance.
///
/// # The difference between `LocalCondVar` and [`CondVar`](crate::sync::CondVar)
///
/// The `LocalCondVar` works with `local tasks`.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// # Example
///
/// ```no_run
/// use orengine::sync::{LocalCondVar, LocalMutex};
/// use orengine::{Local, local_executor, sleep};
/// use std::time::Duration;
///
/// # async fn test() {
/// let cvar = Local::new(LocalCondVar::new());
/// let cvar_clone = cvar.clone();
/// let is_ready = Local::new(LocalMutex::new(false));
/// let is_ready_clone = is_ready.clone();
///
/// local_executor().spawn_local(async move {
///     sleep(Duration::from_secs(1)).await;
///     let mut lock = is_ready_clone.lock().await;
///     *lock = true;
///     lock.unlock();
///     cvar_clone.notify_one();
/// });
///
/// let mut lock = is_ready.lock().await;
/// while !*lock {
///     lock = cvar.wait(lock).await; // wait 1 second
/// }
/// # }
/// ```
pub struct LocalCondVar {
    wait_queue: UnsafeCell<Vec<Task>>,
}

impl LocalCondVar {
    /// Creates a new [`LocalCondVar`].
    #[inline(always)]
    pub fn new() -> LocalCondVar {
        LocalCondVar {
            wait_queue: UnsafeCell::new(Vec::new()),
        }
    }

    /// Wait a notification.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::sync::{LocalCondVar, LocalMutex};
    /// use orengine::{Local, local_executor, sleep};
    /// use std::time::Duration;
    ///
    /// # async fn test() {
    /// let cvar = Local::new(LocalCondVar::new());
    /// let cvar_clone = cvar.clone();
    /// let is_ready = Local::new(LocalMutex::new(false));
    /// let is_ready_clone = is_ready.clone();
    ///
    /// local_executor().spawn_local(async move {
    ///     sleep(Duration::from_secs(1)).await;
    ///     let mut lock = is_ready_clone.lock().await;
    ///     *lock = true;
    ///     lock.unlock();
    ///     cvar_clone.notify_one();
    /// });
    ///
    /// let mut lock = is_ready.lock().await;
    /// while !*lock {
    ///     lock = cvar.wait(lock).await; // wait 1 second
    /// }
    /// # }
    #[inline(always)]
    pub fn wait<'mutex, 'cond_var, T>(
        &'cond_var self,
        local_mutex_guard: LocalMutexGuard<'mutex, T>,
    ) -> WaitCondVar<'mutex, 'cond_var, T> {
        WaitCondVar::new(self, local_mutex_guard.into_local_mutex())
    }

    /// Notifies one waiting task.
    ///
    /// # Attention
    ///
    /// Drop a lock before call [`notify_one`](LocalCondVar::notify_one).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::sync::{LocalMutex, LocalCondVar};
    ///
    /// async fn inc_counter_and_notify(counter: &LocalMutex<i32>, cvar: &LocalCondVar) {
    ///     let mut lock = counter.lock().await;
    ///     *lock += 1;
    ///     lock.unlock();
    ///     cvar.notify_one();
    /// }
    /// ```
    #[inline(always)]
    pub fn notify_one(&self) {
        let wait_queue = unsafe { &mut *self.wait_queue.get() };
        if likely(!wait_queue.is_empty()) {
            let task = wait_queue.pop();
            local_executor().exec_task(unsafe { task.unwrap_unchecked() });
        }
    }

    /// Notifies all waiting tasks.
    ///
    /// # Attention
    ///
    /// Drop a lock before call [`notify_one`](LocalCondVar::notify_one).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::sync::{LocalMutex, LocalCondVar};
    ///
    /// async fn inc_counter_and_notify_all(counter: &LocalMutex<i32>, cvar: &LocalCondVar) {
    ///     let mut lock = counter.lock().await;
    ///     *lock += 1;
    ///     lock.unlock();
    ///     cvar.notify_all();
    /// }
    /// ```
    #[inline(always)]
    pub fn notify_all(&self) {
        let wait_queue = unsafe { &mut *self.wait_queue.get() };
        let executor = local_executor();
        while let Some(task) = wait_queue.pop() {
            executor.exec_task(task);
        }
    }
}

unsafe impl Sync for LocalCondVar {}
impl !Send for LocalCondVar {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::local_executor;
    use crate::sleep::sleep;
    use crate::sync::LocalWaitGroup;
    use std::ops::Deref;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    const TIME_TO_SLEEP: Duration = Duration::from_millis(1);

    async fn test_one(need_drop: bool) {
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
    }

    async fn test_all(need_drop: bool) {
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
    }

    #[orengine_macros::test]
    fn test_one_with_drop_guard() {
        test_one(true).await;
    }

    #[orengine_macros::test]
    fn test_all_with_drop_guard() {
        test_all(true).await;
    }

    #[orengine_macros::test]
    fn test_one_without_drop_guard() {
        test_one(false).await;
    }

    #[orengine_macros::test]
    fn test_all_without_drop_guard() {
        test_all(false).await;
    }
}
