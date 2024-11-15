use std::future::Future;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::panic_if_local_in_future;
use crate::runtime::local_executor;
use crate::sync::{AsyncCondVar, AsyncMutex, AsyncMutexGuard, AsyncSubscribableMutex, Mutex};
use crate::sync_task_queue::SyncTaskList;

/// Current state of the [`WaitCondVar`].
enum WaitState {
    Sleep,
    Wake,
    Lock,
}

/// `WaitCondVar` represents a future returned by the [`CondVar::wait`] method.
///
/// It is used to wait for a notification from a condition variable.
pub struct WaitCondVar<'mutex, 'cond_var, T, Guard>
where
    T: 'mutex + ?Sized,
    Guard: AsyncMutexGuard<'mutex, T>,
    Guard::Mutex: AsyncSubscribableMutex<T>,
{
    state: WaitState,
    cond_var: &'cond_var CondVar,
    mutex: &'mutex Guard::Mutex,
}

impl<'mutex, 'cond_var, T, Guard> WaitCondVar<'mutex, 'cond_var, T, Guard>
where
    T: 'mutex + ?Sized,
    Guard: AsyncMutexGuard<'mutex, T>,
    Guard::Mutex: AsyncSubscribableMutex<T>,
{
    /// Creates a new [`WaitCondVar`].
    #[inline(always)]
    pub fn new(
        cond_var: &'cond_var CondVar,
        mutex: &'mutex Guard::Mutex,
    ) -> Self {
        WaitCondVar {
            state: WaitState::Sleep,
            cond_var,
            mutex,
        }
    }
}

impl<'mutex, 'cond_var, T, Guard> Future for WaitCondVar<'mutex, 'cond_var, T, Guard>
where
    T: 'mutex + ?Sized,
    Guard: AsyncMutexGuard<'mutex, T>,
    Guard::Mutex: AsyncSubscribableMutex<T>,
{
    type Output = <<Guard as AsyncMutexGuard<'mutex, T>>::Mutex as AsyncMutex<T>>::Guard<'mutex>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        panic_if_local_in_future!(cx, "CondVar");

        match this.state {
            WaitState::Sleep => {
                this.state = WaitState::Wake;
                unsafe { local_executor().push_current_task_to(&this.cond_var.wait_queue) };
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

unsafe impl<'mutex, 'cond_var, T, Guard> Send for WaitCondVar<'mutex, 'cond_var, T, Guard>
where
    T: 'mutex + ?Sized + Send,
    Guard: AsyncMutexGuard<'mutex, T> + Send,
    Guard::Mutex: AsyncSubscribableMutex<T>,
{}

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
/// The `CondVar` works with `shared tasks` and can be shared between threads.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// # Example
///
/// ```rust
/// use orengine::sync::{CondVar, Mutex, shared_scope, AsyncMutex, AsyncCondVar};
/// use orengine::sleep;
/// use std::time::Duration;
///
/// # async fn test() {
/// let cvar = CondVar::new();
/// let is_ready = Mutex::new(false);
///
/// shared_scope(|scope| async {
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
pub struct CondVar {
    wait_queue: SyncTaskList,
}

impl CondVar {
    /// Creates a new [`CondVar`].
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            wait_queue: SyncTaskList::new(),
        }
    }
}

impl AsyncCondVar for CondVar {
    type SubscribableMutex<T> = Mutex<T>
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
        WaitCondVar::<
            'mutex,
            '_,
            T,
            <Self::SubscribableMutex<T> as AsyncMutex<T>>::Guard<'mutex>
        >::new(self, guard.mutex())
    }

    #[inline(always)]
    fn notify_one(&self) {
        if let Some(task) = self.wait_queue.pop() {
            local_executor().spawn_shared_task(task);
        }
    }

    #[inline(always)]
    fn notify_all(&self) {
        let executor = local_executor();
        while let Some(task) = self.wait_queue.pop() {
            executor.spawn_shared_task(task);
        }
    }
}

impl Default for CondVar {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Sync for CondVar {}
unsafe impl Send for CondVar {}
impl UnwindSafe for CondVar {}
impl RefUnwindSafe for CondVar {}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use crate::runtime::local_executor;
    use crate::sleep::sleep;
    use crate::sync::{AsyncMutex, WaitGroup};

    use super::*;
    use crate as orengine;
    use crate::test::sched_future_to_another_thread;

    const TIME_TO_SLEEP: Duration = Duration::from_millis(1);

    async fn test_notify_one(need_drop: bool) {
        let start = Instant::now();
        let pair = Arc::new((Mutex::new(false), CondVar::new()));
        let pair2 = pair.clone();

        sched_future_to_another_thread(async move {
            let (lock, cvar) = &*pair2;
            let mut started = lock.lock().await;
            sleep(TIME_TO_SLEEP).await;
            *started = true;
            if need_drop {
                drop(started);
            }
            cvar.notify_one();
        });

        let (lock, cvar) = &*pair;
        let mut started = lock.lock().await;
        while !*started {
            started = cvar.wait(started).await;
        }

        assert!(start.elapsed() >= TIME_TO_SLEEP);
    }

    async fn test_notify_all(need_drop: bool) {
        const NUMBER_OF_WAITERS: usize = 10;

        let start = Instant::now();
        let pair = Arc::new((Mutex::new(false), CondVar::new()));
        let pair2 = pair.clone();
        local_executor().spawn_shared(async move {
            let (lock, cvar) = &*pair2;
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

            sched_future_to_another_thread(async move {
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

    #[orengine_macros::test_shared]
    fn test_shared_cond_var_notify_one_with_drop_guard() {
        test_notify_one(true).await;
    }

    #[orengine_macros::test_shared]
    fn test_shared_cond_var_notify_all_with_drop_guard() {
        test_notify_all(true).await;
    }

    #[orengine_macros::test_shared]
    fn test_shared_cond_var_notify_one_without_drop_guard() {
        test_notify_one(false).await;
    }

    #[orengine_macros::test_shared]
    fn test_shared_cond_var_notify_all_without_drop_guard() {
        test_notify_all(false).await;
    }
}
