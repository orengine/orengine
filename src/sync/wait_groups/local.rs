use crate::get_task_from_context;
use crate::runtime::local_executor;
use crate::runtime::task::Task;
use crate::sync::wait_groups::AsyncWaitGroup;
use std::cell::UnsafeCell;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A [`Future`] to wait for all tasks in the [`LocalWaitGroup`] to complete.
pub struct WaitLocalWaitGroup<'wait_group> {
    wait_group: &'wait_group LocalWaitGroup,
    no_send_marker: std::marker::PhantomData<*const ()>,
}

impl<'wait_group> WaitLocalWaitGroup<'wait_group> {
    /// Creates a new [`WaitLocalWaitGroup`] future.
    #[inline(always)]
    pub fn new(wait_group: &'wait_group LocalWaitGroup) -> Self {
        Self {
            wait_group,
            no_send_marker: std::marker::PhantomData,
        }
    }
}

impl<'wait_group> Future for WaitLocalWaitGroup<'wait_group> {
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

/// `LocalWaitGroup` is a synchronization primitive that allows to [`wait`](Self::wait)
/// until all tasks are [`completed`](Self::done).
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
/// use orengine::sync::{local_scope, AsyncWaitGroup, LocalWaitGroup};
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
///             *number_executed_tasks.borrow_mut() += 1;
///             wait_group.done();
///         });
///     }
///
///     wait_group.wait().await; // wait until all tasks are completed
///     assert_eq!(*number_executed_tasks.borrow(), 10);
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
    pub const fn new() -> Self {
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
    #[allow(clippy::mut_from_ref, reason = "this is local and Sync")]
    fn get_inner(&self) -> &mut Inner {
        unsafe { &mut *self.inner.get() }
    }
}

impl AsyncWaitGroup for LocalWaitGroup {
    #[inline(always)]
    fn add(&self, count: usize) {
        self.get_inner().count += count;
    }

    #[inline(always)]
    fn count(&self) -> usize {
        self.get_inner().count
    }

    #[inline(always)]
    fn done(&self) -> usize {
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

    #[inline(always)]
    #[allow(clippy::future_not_send, reason = "It is `local`")]
    fn wait(&self) -> impl Future<Output = ()> {
        WaitLocalWaitGroup::new(self)
    }
}

impl Default for LocalWaitGroup {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Sync for LocalWaitGroup {}

/// ```compile_fail
/// use orengine::sync::{LocalWaitGroup, AsyncWaitGroup, shared_scope};
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// async fn test() {
///     let wg = LocalWaitGroup::new();
///     let _ = check_send(wg.wait()).await;
/// }
/// ```
#[allow(dead_code, reason = "It is used only in compile tests")]
fn test_compile_local_wait_group() {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::local::Local;
    use crate::runtime::local_executor;
    use crate::yield_now;
    use std::rc::Rc;

    #[orengine::test::test_local]
    fn test_local_wg_many_wait_one() {
        let check_value = Local::new(false);
        let wait_group = Rc::new(LocalWaitGroup::new());
        wait_group.inc();

        for _ in 0..5 {
            let check_value = check_value.clone();
            let wait_group = wait_group.clone();
            local_executor().spawn_local(async move {
                wait_group.wait().await;
                assert!(*check_value.borrow(), "not waited");
            });
        }

        yield_now().await;

        *check_value.borrow_mut() = true;
        wait_group.done();
    }

    #[orengine::test::test_local]
    fn test_local_wg_one_wait_many() {
        let check_value = Local::new(5);
        let wait_group = Rc::new(LocalWaitGroup::new());
        wait_group.add(5);

        for _ in 0..5 {
            let check_value = check_value.clone();
            let wait_group = wait_group.clone();
            local_executor().spawn_local(async move {
                yield_now().await;
                *check_value.borrow_mut() -= 1;
                wait_group.done();
            });
        }

        wait_group.wait().await;
        assert_eq!(*check_value.borrow(), 0, "not waited");
    }
}
