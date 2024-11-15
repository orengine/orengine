use crate::get_task_from_context;
use crate::runtime::local_executor;
use crate::runtime::task::Task;
use crate::sync::mutexes::AsyncSubscribableMutex;
use crate::sync::{AsyncCondVar, AsyncMutex, AsyncMutexGuard, LocalMutex};
use std::cell::UnsafeCell;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Current state of the [`WaitLocalCondVar`].
enum WaitState {
    /// Default state.
    Sleep,
    /// The [`WaitLocalCondVar`] is parked and will be woken up when [`LocalCondVar::notify`] is called.
    Wake,
    /// The [`WaitLocalCondVar`] has been woken up, and it is parked on a [`LocalMutex`],
    /// because the [`LocalMutex`] is locked.
    Lock,
}

/// `WaitLocalCondVar` represents a future returned by the [`LocalCondVar::wait`] method.
///
/// It is used to wait for a notification from a condition variable.
pub struct WaitLocalCondVar<'mutex, 'cond_var, T, Guard>
where
    T: 'mutex + ?Sized,
    Guard: AsyncMutexGuard<'mutex, T>,
    Guard::Mutex: AsyncSubscribableMutex<T>,
{
    state: WaitState,
    cond_var: &'cond_var LocalCondVar,
    mutex: &'mutex Guard::Mutex,
    pd: PhantomData<T>,
}

impl<'mutex, 'cond_var, T, Guard> WaitLocalCondVar<'mutex, 'cond_var, T, Guard>
where
    T: 'mutex + ?Sized,
    Guard: AsyncMutexGuard<'mutex, T>,
    Guard::Mutex: AsyncSubscribableMutex<T>,
{
    /// Creates a new [`WaitLocalCondVar`].
    #[inline(always)]
    pub fn new(
        cond_var: &'cond_var LocalCondVar,
        mutex: &'mutex Guard::Mutex,
    ) -> Self {
        WaitLocalCondVar {
            state: WaitState::Sleep,
            cond_var,
            mutex,
            pd: PhantomData,
        }
    }
}

impl<'mutex, 'cond_var, T, Guard> Future for WaitLocalCondVar<'mutex, 'cond_var, T, Guard>
where
    T: 'mutex + ?Sized,
    Guard: AsyncMutexGuard<'mutex, T>,
    Guard::Mutex: AsyncSubscribableMutex<T>,
{
    type Output = <<Guard as AsyncMutexGuard<'mutex, T>>::Mutex as AsyncMutex<T>>::Guard<'mutex>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        match this.state {
            WaitState::Sleep => {
                this.state = WaitState::Wake;
                let task = unsafe { get_task_from_context!(cx) };
                let wait_queue = unsafe { &mut *this.cond_var.wait_queue.get() };
                wait_queue.push(task);
                Poll::Pending
            }
            WaitState::Wake => {
                if let Some(guard) = this.mutex.try_lock() {
                    Poll::Ready(guard)
                } else {
                    this.state = WaitState::Lock;
                    this.mutex.low_level_subscribe(cx);
                    Poll::Pending
                }
            }
            WaitState::Lock => Poll::Ready(unsafe { this.mutex.get_locked() }),
        }
    }
}

/// `LocalCondVar` is a condition variable that allows tasks to wait until
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
/// ```rust
/// use orengine::sync::{LocalCondVar, LocalMutex, local_scope, AsyncMutex, AsyncCondVar};
/// use orengine::sleep;
/// use std::time::Duration;
///
/// # async fn test() {
/// let cvar = LocalCondVar::new();
/// let is_ready = LocalMutex::new(false);
///
/// local_scope(|scope| async {
///     scope.spawn(async {
///         sleep(Duration::from_secs(1)).await;
///         let mut lock = is_ready.lock().await;
///         *lock = true;
///         drop(lock);
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
pub struct LocalCondVar {
    wait_queue: UnsafeCell<Vec<Task>>,
    // impl !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

impl LocalCondVar {
    /// Creates a new [`LocalCondVar`].
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            wait_queue: UnsafeCell::new(Vec::new()),
            no_send_marker: PhantomData,
        }
    }
}

impl AsyncCondVar for LocalCondVar {
    type SubscribableMutex<T> = LocalMutex<T>
    where
        T: ?Sized;

    #[inline(always)]
    fn wait<'mutex, T>(
        &self,
        guard: <Self::SubscribableMutex<T> as AsyncMutex<T>>::Guard<'mutex>,
    ) -> impl Future<Output=<Self::SubscribableMutex<T> as AsyncMutex<T>>::Guard<'mutex>>
    where
        T: ?Sized + 'mutex,
    {
        WaitLocalCondVar::<
            'mutex,
            '_,
            T,
            <Self::SubscribableMutex<T> as AsyncMutex<T>>::Guard<'mutex>
        >::new(self, guard.mutex())
    }

    #[inline(always)]
    fn notify_one(&self) {
        let wait_queue = unsafe { &mut *self.wait_queue.get() };
        if let Some(task) = wait_queue.pop() {
            local_executor().exec_task(task);
        }
    }

    #[inline(always)]
    fn notify_all(&self) {
        let executor = local_executor();
        let wait_queue = unsafe { &mut *self.wait_queue.get() };
        while let Some(task) = wait_queue.pop() {
            executor.exec_task(task);
        }
    }
}

impl Default for LocalCondVar {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Sync for LocalCondVar {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::runtime::local_executor;
    use crate::sleep::sleep;
    use crate::sync::{AsyncMutex, LocalMutex, LocalWaitGroup};
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    const TIME_TO_SLEEP: Duration = Duration::from_millis(1);

    #[allow(clippy::future_not_send)] // because it is `local`
    async fn test_notify_one(need_drop: bool) {
        let start = Instant::now();
        let pair = Rc::new((LocalMutex::new(false), LocalCondVar::new()));
        let pair2 = pair.clone();
        // Inside our lock, spawn a new thread, and then wait for it to start.
        local_executor().spawn_local(async move {
            let (lock, cvar) = &*pair2;
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
        let (lock, cvar) = &*pair;
        let mut started = lock.lock().await;
        while !*started {
            started = cvar.wait(started).await;
        }

        assert!(start.elapsed() >= TIME_TO_SLEEP);
    }

    #[allow(clippy::future_not_send)] // because it is `local`
    async fn test_notify_all(need_drop: bool) {
        const NUMBER_OF_WAITERS: usize = 10;

        let start = Instant::now();
        let pair = Rc::new((LocalMutex::new(false), LocalCondVar::new()));
        let pair2 = pair.clone();
        // Inside our lock, spawn a new thread, and then wait for it to start.
        local_executor().spawn_local(async move {
            let (lock, cvar) = &*pair2;
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
                let (lock, cvar) = &*pair;
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

    #[orengine_macros::test_local]
    fn test_local_cond_var_notify_one_with_drop_guard() {
        test_notify_one(true).await;
    }

    #[orengine_macros::test_local]
    fn test_local_cond_var_notify_all_with_drop_guard() {
        test_notify_all(true).await;
    }

    #[orengine_macros::test_local]
    fn test_local_cond_var_notify_one_without_drop_guard() {
        test_notify_one(false).await;
    }

    #[orengine_macros::test_local]
    fn test_local_cond_var_notify_all_without_drop_guard() {
        test_notify_all(false).await;
    }
}
