use crate::panic_if_local_in_future;
use crate::runtime::local_executor;
use crate::sync_task_queue::SyncTaskList;
use std::future::Future;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::task::{Context, Poll};

/// A [`Future`] to wait for all tasks in the [`WaitGroup`] to complete.
pub struct Wait<'wait_group> {
    wait_group: &'wait_group WaitGroup,
    was_called: bool,
}

impl<'wait_group> Wait<'wait_group> {
    /// Creates a new [`Wait`] future.
    #[inline(always)]
    pub(crate) fn new(wait_group: &'wait_group WaitGroup) -> Self {
        Self {
            wait_group,
            was_called: false,
        }
    }
}

impl<'wait_group> Future for Wait<'wait_group> {
    type Output = ();

    #[allow(unused)] // because #[cfg(debug_assertions)]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        panic_if_local_in_future!(cx, "WaitGroup");

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
            // So I enqueue the task first, and only then do the check.
            unsafe {
                local_executor().push_current_task_to_and_remove_it_if_counter_is_zero(
                    &this.wait_group.waited_tasks,
                    &this.wait_group.counter,
                    Acquire,
                );
            }

            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

/// `WaitGroup` is a synchronization primitive that allows to wait
/// until all tasks are completed.
///
/// # The difference between `WaitGroup` and [`LocalWaitGroup`](crate::sync::LocalWaitGroup)
///
/// The `WaitGroup` works with `shared tasks` and can be shared between threads.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// # Example
///
/// ```no_run
/// use std::time::Duration;
/// use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
/// use orengine::sleep;
/// use orengine::sync::{shared_scope, WaitGroup};
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
    counter: AtomicUsize,
    waited_tasks: SyncTaskList,
}

impl WaitGroup {
    /// Creates a new `WaitGroup`.
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
            waited_tasks: SyncTaskList::new(),
        }
    }

    /// Adds `count` to the `WaitGroup` counter.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    /// use orengine::sleep;
    /// use orengine::sync::{shared_scope, WaitGroup};
    ///
    /// # async fn foo() {
    /// let wait_group = WaitGroup::new();
    /// let number_executed_tasks = AtomicUsize::new(0);
    ///
    /// shared_scope(|scope| async {
    ///     wait_group.add(10);
    ///     for i in 0..10 {
    ///         scope.spawn(async {
    ///             sleep(Duration::from_millis(i)).await;
    ///             number_executed_tasks.fetch_add(1, SeqCst);
    ///             wait_group.done();
    ///         });
    ///     }
    ///
    ///     wait_group.wait().await; // wait until 10 tasks are completed
    ///     assert_eq!(number_executed_tasks.load(SeqCst), 10);
    /// }).await;
    /// # }
    /// ```
    #[inline(always)]
    pub fn add(&self, count: usize) {
        self.counter.fetch_add(count, Acquire);
    }

    /// Adds 1 to the `WaitGroup` counter.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    /// use orengine::sleep;
    /// use orengine::sync::{shared_scope, WaitGroup};
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
    #[inline(always)]
    pub fn inc(&self) {
        self.add(1);
    }

    /// Returns the `WaitGroup` counter.
    ///
    /// Example
    ///
    /// ```no_run
    /// use orengine::sync::WaitGroup;
    ///
    /// # async fn foo() {
    /// let wait_group = WaitGroup::new();
    /// assert_eq!(wait_group.count(), 0);
    /// wait_group.inc();
    /// assert_eq!(wait_group.count(), 1);
    /// wait_group.done();
    /// assert_eq!(wait_group.count(), 0);
    /// # }
    /// ```
    #[inline(always)]
    pub fn count(&self) -> usize {
        self.counter.load(Acquire)
    }

    /// Decreases the `WaitGroup` counter by 1 and wakes up all tasks that are waiting
    /// if the counter reaches 0.
    ///
    /// Returns the previous value of the counter.
    ///
    /// # Safety
    ///
    /// The counter must be greater than 0.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::sync::{shared_scope, WaitGroup};
    ///
    /// # async fn foo() {
    /// let wait_group = WaitGroup::new();
    ///
    /// shared_scope(|scope| async {
    ///     wait_group.inc();
    ///     scope.spawn(async {
    ///         // wake up the waiting task, because a current and the only one task is done
    ///         wait_group.done();
    ///     });
    ///
    ///     wait_group.wait().await; // wait until all tasks are completed
    /// }).await;
    /// # }
    /// ```
    #[inline(always)]
    pub fn done(&self) -> usize {
        let prev_count = self.counter.fetch_sub(1, Release);
        debug_assert!(
            prev_count > 0,
            "WaitGroup::done called after counter reached 0"
        );

        if prev_count == 1 {
            let executor = local_executor();
            let mut tasks = Vec::new();
            self.waited_tasks.pop_all_in(&mut tasks);
            for task in tasks {
                executor.spawn_shared_task(task);
            }
        }

        prev_count
    }

    /// Waits until the `WaitGroup` counter reaches 0.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    /// use orengine::sleep;
    /// use orengine::sync::{shared_scope, WaitGroup};
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
    #[inline(always)]
    pub async fn wait(&self) {
        Wait::new(self).await
    }
}

unsafe impl Sync for WaitGroup {}
unsafe impl Send for WaitGroup {}
impl UnwindSafe for WaitGroup {}
impl RefUnwindSafe for WaitGroup {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::test::sched_future_to_another_thread;
    use crate::{sleep, yield_now};
    use std::sync::Arc;
    use std::time::Duration;

    const PAR: usize = 10;

    #[orengine_macros::test_shared]
    fn test_shared_wg_many_wait_one() {
        let check_value = Arc::new(std::sync::Mutex::new(false));
        let wait_group = Arc::new(WaitGroup::new());
        wait_group.inc();

        for _ in 0..PAR {
            let check_value = check_value.clone();
            let wait_group = wait_group.clone();

            sched_future_to_another_thread(async move {
                wait_group.wait().await;
                if !*check_value.lock().unwrap() {
                    panic!("not waited");
                }
            });
        }

        yield_now().await;

        *check_value.lock().unwrap() = true;
        wait_group.done();
    }

    #[orengine_macros::test_shared]
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
        if *check_value.lock().unwrap() != 0 {
            panic!("not waited");
        }
    }

    #[orengine_macros::test_shared]
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
        if *check_value.lock().unwrap() != 0 {
            panic!("not waited");
        }
    }
}
