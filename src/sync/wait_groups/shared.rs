use crate::panic_if_local_in_future;
use crate::runtime::call::Call;
use crate::runtime::local_executor;
use crate::sync::wait_groups::AsyncWaitGroup;
use crate::utils::{
    acquire_sync_task_list_from_pool, acquire_task_vec_from_pool, SyncTaskListFromPool,
};
use crossbeam::utils::CachePadded;
use std::future::Future;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::task::{Context, Poll};

/// A [`Future`] to wait for all tasks in the [`WaitGroup`] to complete.
#[repr(C)]
pub struct WaitSharedWaitGroup<'wait_group> {
    wait_group: &'wait_group WaitGroup,
    was_called: bool,
}

impl<'wait_group> WaitSharedWaitGroup<'wait_group> {
    /// Creates a new [`WaitSharedWaitGroup`] future.
    #[inline]
    pub(crate) fn new(wait_group: &'wait_group WaitGroup) -> Self {
        Self {
            wait_group,
            was_called: false,
        }
    }
}

impl Future for WaitSharedWaitGroup<'_> {
    type Output = ();

    #[allow(unused, reason = "Here we use #[cfg(debug_assertions)].")]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = &mut *self;
        unsafe { panic_if_local_in_future!(cx, "WaitGroup") };

        if !this.was_called {
            this.was_called = true;

            // Here I need to explain this decision.
            //
            // I think it's a rare case where all the tasks were completed before the wait was called.
            // Therefore, I sacrifice performance in this case to get much faster in the frequent case.
            //
            // Otherwise, I'd have to keep track of how many tasks are in the queue,
            // which means calling out one more atomic operation in each done call.
            //
            // So, I enqueue the task first, and only then do the check.
            unsafe {
                local_executor().invoke_call(Call::PushCurrentTaskToAndRemoveItIfCounterIsZero(
                    &*this.wait_group.waited_tasks,
                    &*this.wait_group.counter,
                    Acquire,
                ));
            }

            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

/// `WaitGroup` is a synchronization primitive that allows to [`wait`](Self::wait)
/// until all tasks are [`completed`](Self::done).
///
/// # The difference between `WaitGroup` and [`LocalWaitGroup`](crate::sync::LocalWaitGroup)
///
/// The `WaitGroup` works with `shared tasks` and can be shared between threads.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// # Example
///
/// ```rust
/// use std::time::Duration;
/// use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
/// use orengine::sleep;
/// use orengine::sync::{shared_scope, AsyncWaitGroup, WaitGroup};
///
/// # async fn foo() {
/// let wait_group = WaitGroup::new();
/// let number_executed_tasks = AtomicUsize::new(0);
///
/// shared_scope(|scope| async {
///     for i in 0..10 {
///         wait_group.inc();
///         scope.spawn(async {
///             sleep(Duration::from_millis(i)).await;
///             number_executed_tasks.fetch_add(1, SeqCst);
///             wait_group.done();
///         });
///     }
///
///     wait_group.wait().await; // wait until all tasks are completed
///     assert_eq!(number_executed_tasks.load(SeqCst), 10);
/// }).await;
/// # }
/// ```
pub struct WaitGroup {
    counter: CachePadded<AtomicUsize>,
    waited_tasks: SyncTaskListFromPool,
}

impl WaitGroup {
    /// Creates a new `WaitGroup`.
    pub fn new() -> Self {
        Self {
            counter: CachePadded::new(AtomicUsize::new(0)),
            waited_tasks: acquire_sync_task_list_from_pool(),
        }
    }
}

impl AsyncWaitGroup for WaitGroup {
    #[inline]
    fn add(&self, count: usize) {
        self.counter.fetch_add(count, Acquire);
    }

    #[inline]
    fn count(&self) -> usize {
        self.counter.load(Acquire)
    }

    #[inline]
    fn done(&self) -> usize {
        let prev_count = self.counter.fetch_sub(1, Release);
        debug_assert!(
            prev_count > 0,
            "WaitGroup::done called after counter reached 0"
        );

        if prev_count == 1 {
            let executor = local_executor();
            let mut tasks = acquire_task_vec_from_pool();

            self.waited_tasks.pop_all_in(&mut tasks);
            for task in tasks.drain(..) {
                executor.spawn_shared_task(task);
            }
        }

        prev_count
    }

    #[inline]
    fn wait(&self) -> impl Future<Output = ()> {
        WaitSharedWaitGroup::new(self)
    }
}

impl Default for WaitGroup {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Sync for WaitGroup {}
unsafe impl Send for WaitGroup {}
impl UnwindSafe for WaitGroup {}
impl RefUnwindSafe for WaitGroup {}

/// ```rust
/// use orengine::sync::{WaitGroup, shared_scope, AsyncWaitGroup};
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// async fn test() {
///     let wg = WaitGroup::new();
///     let _ = check_send(wg.wait()).await;
/// }
/// ```
#[allow(dead_code, reason = "It is used only in compile tests")]
fn test_compile_shared_wait_group() {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::sync::{AsyncMutex, Mutex};
    use crate::test::sched_future_to_another_thread;
    use crate::{sleep, yield_now};
    use std::sync::Arc;
    use std::time::Duration;

    const PAR: usize = 10;

    #[orengine::test::test_shared]
    fn test_shared_wg_many_wait_one() {
        let check_value = Arc::new(std::sync::Mutex::new(false));
        let wait_group = Arc::new(WaitGroup::new());
        wait_group.inc();

        for _ in 0..PAR {
            let check_value = check_value.clone();
            let wait_group = wait_group.clone();

            sched_future_to_another_thread(async move {
                wait_group.wait().await;
                assert!(*check_value.lock().unwrap(), "not waited");
            });
        }

        yield_now().await;

        *check_value.lock().unwrap() = true;
        wait_group.done();
    }

    #[orengine::test::test_shared]
    fn test_shared_wg_one_wait_many_task_finished_after_wait() {
        let check_value = Arc::new(std::sync::Mutex::new(PAR));
        let wait_group = Arc::new(WaitGroup::new());
        wait_group.add(PAR);

        for _ in 0..PAR {
            let check_value = check_value.clone();
            let wait_group = wait_group.clone();

            sched_future_to_another_thread(async move {
                *check_value.lock().unwrap() -= 1;
                sleep(Duration::from_millis(100)).await;
                wait_group.done();
            });
        }

        wait_group.wait().await;
        assert_eq!(*check_value.lock().unwrap(), 0, "not waited");
    }

    #[orengine::test::test_shared]
    fn test_shared_wg_one_wait_many_task_finished_before_wait() {
        let check_value = Arc::new(std::sync::Mutex::new(PAR));
        let wait_group = Arc::new(WaitGroup::new());
        wait_group.add(PAR);

        for _ in 0..PAR {
            let check_value = check_value.clone();
            let wait_group = wait_group.clone();

            sched_future_to_another_thread(async move {
                *check_value.lock().unwrap() -= 1;
                wait_group.done();
            });
        }

        wait_group.wait().await;
        assert_eq!(*check_value.lock().unwrap(), 0, "not waited");
    }

    #[orengine::test::test_shared]
    fn test_shared_wg_as_barrier() {
        let check_value = Arc::new(Mutex::new(0));
        let wait_group = Arc::new(WaitGroup::new());
        wait_group.add(6);

        for _ in 0..5 {
            let check_value = check_value.clone();
            let wait_group = wait_group.clone();

            local_executor().spawn_shared(async move {
                yield_now().await;

                *check_value.lock().await += 1;

                wait_group.done();
                wait_group.wait().await;

                assert_eq!(*check_value.lock().await, 6);
            });
        }

        yield_now().await;

        *check_value.lock().await += 1;

        wait_group.done();
        wait_group.wait().await;

        assert_eq!(*check_value.lock().await, 6);
    }
}
