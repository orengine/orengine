use crate::get_task_from_context;
use crate::runtime::local_executor;
use crate::runtime::task::Task;
use std::cell::UnsafeCell;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A [`Future`] to wait for all tasks in the [`WaitGroup`] to complete.
pub struct Wait<'wait_group> {
    wait_group: &'wait_group LocalWaitGroup,
}

impl<'wait_group> Wait<'wait_group> {
    /// Creates a new [`Wait`] future.
    #[inline(always)]
    pub fn new(wait_group: &'wait_group LocalWaitGroup) -> Self {
        Self { wait_group }
    }
}

impl<'wait_group> Future for Wait<'wait_group> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let inner = this.wait_group.get_inner();
        if inner.count != 0 {
            let task = unsafe { get_task_from_context!(cx) };
            inner.waited_tasks.push(task);

            return Poll::Pending;
        }

        Poll::Ready(())
    }
}

/// Inner structure of [`LocalWaitGroup`] for internal use via [`UnsafeCell`].
struct Inner {
    count: usize,
    waited_tasks: Vec<Task>,
}

/// `LocalWaitGroup` is a synchronization primitive that allows to wait
/// until all tasks are completed.
///
/// # The difference between `LocalWaitGroup` and [`WaitGroup`](crate::sync::WaitGroup)
///
/// The `LocalWaitGroup` works with `local tasks`.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// # Example
///
/// ```rust
/// use std::time::Duration;
/// use orengine::{sleep, Local};
/// use orengine::sync::{local_scope, LocalWaitGroup};
///
/// # async fn foo() {
/// let wait_group = LocalWaitGroup::new();
/// let number_executed_tasks = Local::new(0);
///
/// local_scope(|scope| async {
///     for i in 0..10 {
///         wait_group.inc();
///         scope.spawn(async {
///             sleep(Duration::from_millis(i)).await;
///             *number_executed_tasks.get_mut() += 1;
///             wait_group.done();
///         });
///     }
///
///     wait_group.wait().await; // wait until all tasks are completed
///     assert_eq!(*number_executed_tasks, 10);
/// }).await;
/// # }
/// ```
pub struct LocalWaitGroup {
    inner: UnsafeCell<Inner>,
    // impl !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

impl LocalWaitGroup {
    /// Creates a new `LocalWaitGroup`.
    pub fn new() -> Self {
        Self {
            inner: UnsafeCell::new(Inner {
                count: 0,
                waited_tasks: Vec::new(),
            }),
            no_send_marker: std::marker::PhantomData,
        }
    }

    /// Returns a mutable reference to the [`Inner`].
    #[inline(always)]
    #[allow(clippy::mut_from_ref, reason = "this is local and unsafe")]
    fn get_inner(&self) -> &mut Inner {
        unsafe { &mut *self.inner.get() }
    }

    /// Adds `count` to the `LocalWaitGroup` counter.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use orengine::{sleep, Local};
    /// use orengine::sync::{local_scope, LocalWaitGroup};
    ///
    /// # async fn foo() {
    /// let wait_group = LocalWaitGroup::new();
    /// let number_executed_tasks = Local::new(0);
    ///
    /// local_scope(|scope| async {
    ///     wait_group.add(10);
    ///     for i in 0..10 {
    ///         scope.spawn(async {
    ///             sleep(Duration::from_millis(i)).await;
    ///             *number_executed_tasks.get_mut() += 1;
    ///             wait_group.done();
    ///         });
    ///     }
    ///
    ///     wait_group.wait().await; // wait until 10 tasks are completed
    ///     assert_eq!(*number_executed_tasks, 10);
    /// }).await;
    /// # }
    /// ```
    #[inline(always)]
    pub fn add(&self, count: usize) {
        self.get_inner().count += count;
    }

    /// Adds 1 to the `LocalWaitGroup` counter.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use orengine::{sleep, Local};
    /// use orengine::sync::{local_scope, LocalWaitGroup};
    ///
    /// # async fn foo() {
    /// let wait_group = LocalWaitGroup::new();
    /// let number_executed_tasks = Local::new(0);
    ///
    /// local_scope(|scope| async {
    ///     for i in 0..10 {
    ///         wait_group.inc();
    ///         scope.spawn(async {
    ///             sleep(Duration::from_millis(i)).await;
    ///             *number_executed_tasks.get_mut() += 1;
    ///             wait_group.done();
    ///         });
    ///     }
    ///
    ///     wait_group.wait().await; // wait until all tasks are completed
    ///     assert_eq!(*number_executed_tasks, 10);
    /// }).await;
    /// # }
    /// ```
    #[inline(always)]
    pub fn inc(&self) {
        self.add(1);
    }

    /// Returns the `LocalWaitGroup` counter.
    ///
    /// Example
    ///
    /// ```rust
    /// use orengine::sync::LocalWaitGroup;
    ///
    /// # async fn foo() {
    /// let wait_group = LocalWaitGroup::new();
    /// assert_eq!(wait_group.count(), 0);
    /// wait_group.inc();
    /// assert_eq!(wait_group.count(), 1);
    /// wait_group.done();
    /// assert_eq!(wait_group.count(), 0);
    /// # }
    /// ```
    #[inline(always)]
    pub fn count(&self) -> usize {
        self.get_inner().count
    }

    /// Decreases the `LocalWaitGroup` counter by 1 and wakes up all tasks that are waiting
    /// if the counter reaches 0.
    ///
    /// Returns the current value of the counter.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::sync::{local_scope, LocalWaitGroup};
    ///
    /// # async fn foo() {
    /// let wait_group = LocalWaitGroup::new();
    ///
    /// local_scope(|scope| async {
    ///     wait_group.inc();
    ///     scope.spawn(async {
    ///         // wake up the waiting task, because a current and the only one task is done
    ///         let count = wait_group.done();
    ///         assert_eq!(count, 0);
    ///     });
    ///
    ///     wait_group.wait().await; // wait until all tasks are completed
    /// }).await;
    /// # }
    /// ```
    #[inline(always)]
    pub fn done(&self) -> usize {
        let inner = self.get_inner();
        inner.count -= 1;
        if inner.count == 0 {
            let executor = local_executor();

            for task in inner.waited_tasks.drain(..) {
                executor.exec_task(task);
            }
        }

        inner.count
    }

    /// Waits until the `LocalWaitGroup` counter reaches 0.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use orengine::{sleep, Local};
    /// use orengine::sync::{local_scope, LocalWaitGroup};
    ///
    /// # async fn foo() {
    /// let wait_group = LocalWaitGroup::new();
    /// let number_executed_tasks = Local::new(0);
    ///
    /// local_scope(|scope| async {
    ///     for i in 0..10 {
    ///         wait_group.inc();
    ///         scope.spawn(async {
    ///             sleep(Duration::from_millis(i)).await;
    ///             *number_executed_tasks.get_mut() += 1;
    ///             wait_group.done();
    ///         });
    ///     }
    ///
    ///     wait_group.wait().await; // wait until all tasks are completed
    ///     assert_eq!(*number_executed_tasks, 10);
    /// }).await;
    /// # }
    /// ```
    #[inline(always)]
    #[must_use = "Future must be awaited to start the wait"]
    pub fn wait(&self) -> Wait {
        Wait::new(self)
    }
}

impl Default for LocalWaitGroup {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Sync for LocalWaitGroup {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::local::Local;
    use crate::runtime::local_executor;
    use crate::yield_now;
    use std::rc::Rc;

    #[orengine_macros::test_local]
    fn test_local_wg_many_wait_one() {
        let check_value = Local::new(false);
        let wait_group = Rc::new(LocalWaitGroup::new());
        wait_group.inc();

        for _ in 0..5 {
            let check_value = check_value.clone();
            let wait_group = wait_group.clone();
            local_executor().spawn_local(async move {
                wait_group.wait().await;
                assert!(*check_value, "not waited");
            });
        }

        yield_now().await;

        *check_value.get_mut() = true;
        wait_group.done();
    }

    #[orengine_macros::test_local]
    fn test_local_wg_one_wait_many() {
        let check_value = Local::new(5);
        let wait_group = Rc::new(LocalWaitGroup::new());
        wait_group.add(5);

        for _ in 0..5 {
            let check_value = check_value.clone();
            let wait_group = wait_group.clone();
            local_executor().spawn_local(async move {
                yield_now().await;
                *check_value.get_mut() -= 1;
                wait_group.done();
            });
        }

        wait_group.wait().await;
        assert_eq!(*check_value, 0, "not waited");
    }
}
