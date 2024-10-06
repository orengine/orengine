use crate::panic_if_local_in_future;
use crate::runtime::local_executor;
use crate::sync::WaitGroup;
use std::future::Future;
use std::pin::Pin;
use std::task::Poll;

/// A scope to spawn scoped global tasks in.
///
/// # The difference between `Scope` and [`LocalScope`](crate::sync::LocalScope)
///
/// The `Scope` works with `global tasks` and its tasks can be shared between threads.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// See [`global_scope`] for details.
pub struct Scope<'scope> {
    wg: WaitGroup,
    _scope: std::marker::PhantomData<&'scope ()>,
}

impl<'scope> Scope<'scope> {
    /// Executes a new global task within a scope.
    ///
    /// Unlike non-scoped tasks, tasks created with this function may
    /// borrow non-`'static` data from the outside the scope. See [`global_scope`] for
    /// details.
    ///
    /// The created task will be executed immediately.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    /// use std::time::Duration;
    /// use orengine::sleep;
    /// use orengine::sync::{global_scope, WaitGroup};
    ///
    /// # async fn foo() {
    /// let wg = WaitGroup::new();
    /// let a = AtomicUsize::new(0);
    ///
    /// global_scope(|scope| async {
    ///     for i in 0..10 {
    ///         wg.inc();
    ///         scope.exec(async {
    ///             assert_eq!(a.load(SeqCst), i);
    ///             a.fetch_add(1, SeqCst);
    ///             sleep(Duration::from_millis(i as u64)).await;
    ///             wg.done();
    ///         });
    ///     }
    ///
    ///     wg.wait().await;
    ///     assert_eq!(a.load(SeqCst), 10);
    /// }).await;
    ///
    /// assert_eq!(a.load(SeqCst), 10);
    /// # }
    /// ```
    #[inline(always)]
    pub fn exec<F: Future<Output = ()> + Send>(&'scope self, future: F) {
        self.wg.inc();
        let handle = ScopedHandle {
            scope: self,
            fut: future,
        };

        let global_task = crate::runtime::Task::from_future(handle, 0);
        local_executor().exec_task(global_task);
    }

    /// Spawns a new global task within a scope.
    ///
    /// Unlike non-scoped tasks, tasks created with this function may
    /// borrow non-`'static` data from the outside the scope. See [`global_scope`] for
    /// details.
    ///
    /// The created task will be executed later.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    /// use orengine::sync::{global_scope, WaitGroup};
    ///
    /// # async fn foo() {
    /// let wg = WaitGroup::new();
    /// let a = AtomicUsize::new(0);
    ///
    /// global_scope(|scope| async {
    ///     for i in 0..10 {
    ///         wg.inc();
    ///         scope.spawn(async {
    ///             a.fetch_add(1, SeqCst);
    ///             wg.done();
    ///         });
    ///     }
    ///
    ///     assert_eq!(a.load(SeqCst), 0);
    ///     wg.wait().await;
    ///     assert_eq!(a.load(SeqCst), 10);
    /// }).await;
    ///
    /// assert_eq!(a.load(SeqCst), 10);
    /// # }
    /// ```
    #[inline(always)]
    pub fn spawn<F: Future<Output = ()> + Send>(&'scope self, future: F) {
        self.wg.inc();
        let handle = ScopedHandle {
            scope: self,
            fut: future,
        };

        local_executor().spawn_global(handle);
    }
}

unsafe impl Send for Scope<'_> {}
unsafe impl Sync for Scope<'_> {}

/// `ScopedHandle` is a wrapper of `Future<Output = ()>`
/// to decrement the wait group when the future is done.
pub(crate) struct ScopedHandle<'scope, Fut: Future<Output = ()> + Send> {
    scope: &'scope Scope<'scope>,
    fut: Fut,
}

impl<'scope, Fut: Future<Output = ()> + Send> Future for ScopedHandle<'scope, Fut> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        panic_if_local_in_future!(cx, "Scope");
        let this = unsafe { self.get_unchecked_mut() };

        let mut pinned_future = unsafe { Pin::new_unchecked(&mut this.fut) };
        match pinned_future.as_mut().poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(()) => {
                this.scope.wg.done();
                Poll::Ready(())
            }
        }
    }
}

unsafe impl<F: Future<Output = ()> + Send> Send for ScopedHandle<'_, F> {}
unsafe impl<F: Future<Output = ()> + Send> Sync for ScopedHandle<'_, F> {}

/// Creates a [`global scope`](GlobalScope) for spawning scoped global tasks.
///
/// The function passed to `scope` will be provided a [`Scope`] object,
/// through which scoped global tasks can be [spawned][`Scope::spawn`]
/// or [executed][`Scope::exec`].
///
/// Unlike non-scoped tasks, scoped tasks can borrow non-`'static` data,
/// as the scope guarantees all tasks will be awaited at the end of the scope.
///
/// # The difference between `global_scope` and [`local_scope`](crate::sync::local_scope)
///
/// The `global_scope` works with `global tasks` and its tasks can be shared between threads.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// # Example
///
/// ```no_run
/// use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
/// use std::time::Duration;
/// use orengine::sleep;
/// use orengine::sync::{global_scope, WaitGroup};
///
/// # async fn foo() {
/// let wg = WaitGroup::new();
/// let a = AtomicUsize::new(0);
///
/// global_scope(|scope| async {
///     for i in 0..10 {
///         wg.inc();
///         scope.exec(async {
///             assert_eq!(a.load(SeqCst), i);
///             a.fetch_add(1, SeqCst);
///             sleep(Duration::from_millis(i as u64)).await;
///             wg.done();
///         });
///     }
///
///     wg.wait().await;
///     assert_eq!(a.load(SeqCst), 10);
/// }).await;
///
/// assert_eq!(a.load(SeqCst), 10);
/// # }
/// ```
#[inline(always)]
pub async fn global_scope<'scope, Fut, F>(f: F)
where
    Fut: Future<Output = ()> + Send,
    F: FnOnce(&'scope Scope<'scope>) -> Fut,
{
    let scope = Scope {
        wg: WaitGroup::new(),
        _scope: std::marker::PhantomData,
    };
    let static_scope = unsafe { std::mem::transmute::<&_, &'static Scope<'static>>(&scope) };

    f(static_scope).await;

    let _ = scope.wg.wait().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{sleep, yield_now};
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::SeqCst;
    use std::time::Duration;

    #[orengine_macros::test_global]
    fn test_global_scope_exec() {
        let a = AtomicUsize::new(0);

        global_scope(|scope| async {
            scope.exec(async {
                assert_eq!(a.load(SeqCst), 0);
                a.fetch_add(1, SeqCst);
                yield_now().await;
                assert_eq!(a.load(SeqCst), 3);
                a.fetch_add(1, SeqCst);
            });

            scope.exec(async {
                assert_eq!(a.load(SeqCst), 1);
                a.fetch_add(1, SeqCst);
                sleep(Duration::from_millis(1)).await;
                assert_eq!(a.load(SeqCst), 4);
                a.fetch_add(1, SeqCst);
            });

            assert_eq!(a.load(SeqCst), 2);
            a.fetch_add(1, SeqCst);
        })
        .await;

        assert_eq!(a.load(SeqCst), 5);
    }

    #[orengine_macros::test_global]
    fn test_global_scope_spawn() {
        let a = AtomicUsize::new(0);

        global_scope(|scope| async {
            scope.spawn(async {
                assert_eq!(a.load(SeqCst), 2);
                a.fetch_add(1, SeqCst);
                yield_now().await;
                assert_eq!(a.load(SeqCst), 3);
                a.fetch_add(1, SeqCst);
            });

            scope.spawn(async {
                assert_eq!(a.load(SeqCst), 1);
                a.fetch_add(1, SeqCst);
                sleep(Duration::from_millis(1)).await;
                assert_eq!(a.load(SeqCst), 4);
                a.fetch_add(1, SeqCst);
            });

            assert_eq!(a.load(SeqCst), 0);
            a.fetch_add(1, SeqCst);
        })
        .await;

        assert_eq!(a.load(SeqCst), 5);
    }
}
