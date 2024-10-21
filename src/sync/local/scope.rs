use crate::runtime::{local_executor, Locality};
use crate::sync::LocalWaitGroup;
use crate::yield_now;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::Poll;

/// A scope to spawn scoped local tasks in.
///
/// # The difference between `LocalScope` and [`Scope`](crate::sync::Scope)
///
/// The `LocalScope` works with `local tasks`.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// See [`local_scope`] for details.
pub struct LocalScope<'scope> {
    wg: LocalWaitGroup,
    _scope: PhantomData<&'scope ()>,
    no_send_marker: PhantomData<*mut ()>,
}

impl<'scope> LocalScope<'scope> {
    /// Executes a new local task within a scope.
    ///
    /// Unlike non-scoped tasks, tasks created with this function may
    /// borrow non-`'static` data from the outside the scope. See [`local_scope`] for
    /// details.
    ///
    /// The created task will be executed immediately.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::ops::Deref;
    /// use std::time::Duration;
    /// use orengine::{sleep, Local};
    /// use orengine::sync::{local_scope, LocalWaitGroup};
    ///
    /// # async fn foo() {
    /// let wg = LocalWaitGroup::new();
    /// let a = Local::new(0);
    ///
    /// local_scope(|scope| async {
    ///     for i in 0..10 {
    ///         wg.inc();
    ///         scope.exec(async {
    ///             assert_eq!(*a.deref(), i);
    ///             *a.get_mut() += 1;
    ///             sleep(Duration::from_millis(i)).await;
    ///             wg.done();
    ///         });
    ///     }
    ///
    ///     wg.wait().await;
    ///     assert_eq!(*a.deref(), 10);
    /// }).await;
    ///
    /// assert_eq!(*a.deref(), 10);
    /// # }
    /// ```
    #[inline(always)]
    pub fn exec<F: Future<Output = ()>>(&'scope self, future: F) {
        self.wg.inc();
        let handle = LocalScopedHandle {
            scope: self,
            fut: future,
            no_send_marker: PhantomData,
        };

        let local_task = crate::runtime::Task::from_future(handle, Locality::local());
        local_executor().exec_task(local_task);
    }

    /// Spawns a new local task within a scope.
    ///
    /// Unlike non-scoped tasks, tasks spawned with this function may
    /// borrow non-`'static` data from the outside the scope. See [`local_scope`] for
    /// details.
    ///
    /// The spawned task will be executed later.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::ops::Deref;
    /// use orengine::Local;
    /// use orengine::sync::{local_scope, LocalWaitGroup};
    ///
    /// # async fn foo() {
    /// let wg = LocalWaitGroup::new();
    /// let a = Local::new(0);
    ///
    /// local_scope(|scope| async {
    ///     for i in 0..10 {
    ///         wg.inc();
    ///         scope.spawn(async {
    ///             *a.get_mut() += 1;
    ///             wg.done();
    ///         });
    ///     }
    ///
    ///     assert_eq!(*a.deref(), 0);
    ///     wg.wait().await;
    ///     assert_eq!(*a.deref(), 10);
    /// }).await;
    ///
    /// assert_eq!(*a.deref(), 10);
    /// # }
    /// ```
    #[inline(always)]
    pub fn spawn<F: Future<Output = ()>>(&'scope self, future: F) {
        self.wg.inc();
        let handle = LocalScopedHandle {
            scope: self,
            fut: future,
            no_send_marker: PhantomData,
        };

        local_executor().spawn_local(handle);
    }
}

/// `LocalScopedHandle` is a wrapper of `Future<Output = ()>`
/// to decrement the wait group when the future is done.
pub(crate) struct LocalScopedHandle<'scope, Fut: Future<Output = ()>> {
    scope: &'scope LocalScope<'scope>,
    fut: Fut,
    // impl !Send
    no_send_marker: PhantomData<*const ()>,
}

impl<'scope, Fut: Future<Output = ()>> Future for LocalScopedHandle<'scope, Fut> {
    type Output = ();

    #[inline(always)]
    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
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

/// Creates a [`local scope`](LocalScope) for spawning scoped local tasks.
///
/// The function passed to `scope` will be provided a [`LocalScope`] object,
/// through which scoped local tasks can be [spawned][`LocalScope::spawn`]
/// or [executed][`LocalScope::exec`].
///
/// Unlike non-scoped tasks, scoped tasks can borrow non-`'static` data,
/// as the scope guarantees all tasks will be awaited at the end of the scope.
///
/// # The difference between `local_scope` and [`global_scope`](crate::sync::global_scope)
///
/// The `local_scope` works with `local tasks`.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// # Example
///
/// ```no_run
/// use std::ops::Deref;
/// use std::time::Duration;
/// use orengine::{sleep, Local};
/// use orengine::sync::{local_scope, LocalWaitGroup};
///
/// # async fn foo() {
/// let wg = LocalWaitGroup::new();
/// let a = Local::new(0);
///
/// local_scope(|scope| async {
///     for i in 0..10 {
///         wg.inc();
///         scope.exec(async {
///             assert_eq!(*a.deref(), i);
///             *a.get_mut() += 1;
///             sleep(Duration::from_millis(i)).await;
///             wg.done();
///         });
///     }
///
///     wg.wait().await;
///     assert_eq!(*a.deref(), 10);
/// }).await;
///
/// assert_eq!(*a.deref(), 10);
/// # }
/// ```
#[inline(always)]
pub async fn local_scope<'scope, Fut, F>(f: F)
where
    Fut: Future<Output = ()>,
    F: FnOnce(&'scope LocalScope<'scope>) -> Fut,
{
    let scope = LocalScope {
        wg: LocalWaitGroup::new(),
        _scope: PhantomData,
        no_send_marker: PhantomData,
    };
    let static_scope = unsafe { std::mem::transmute::<&_, &'static LocalScope<'static>>(&scope) };

    f(static_scope).await;

    let _ = static_scope.wg.wait().await;

    yield_now().await // FIXME: you can't call 2 local_scopes in the same task if you don't yield
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::local::Local;
    use crate::{sleep, yield_now};
    use std::ops::Deref;
    use std::time::Duration;

    #[orengine_macros::test_local]
    fn test_scope_exec() {
        let local_a = Local::new(0);

        local_scope(|scope| async {
            scope.exec(async {
                assert_eq!(*local_a.deref(), 0);
                *local_a.get_mut() += 1;
                yield_now().await;
                assert_eq!(*local_a.deref(), 2);
                *local_a.get_mut() += 1;
            });

            scope.exec(async {
                assert_eq!(*local_a.deref(), 1);
                *local_a.get_mut() += 1;
                yield_now().await;
                assert_eq!(*local_a.deref(), 3);
                *local_a.get_mut() += 1;
            });
        })
        .await;

        assert_eq!(*local_a.deref(), 4);

        let local_a = Local::new(0);

        local_scope(|scope| async {
            scope.exec(async {
                assert_eq!(*local_a.deref(), 0);
                *local_a.get_mut() += 1;
                yield_now().await;
                assert_eq!(*local_a.deref(), 3);
                *local_a.get_mut() += 1;
            });

            scope.exec(async {
                assert_eq!(*local_a.deref(), 1);
                *local_a.get_mut() += 1;
                yield_now().await;
                assert_eq!(*local_a.deref(), 4);
                *local_a.get_mut() += 1;
            });

            assert_eq!(*local_a.deref(), 2);
            *local_a.get_mut() += 1;
        })
        .await;

        assert_eq!(*local_a.deref(), 5);
    }

    #[orengine_macros::test_local]
    fn test_scope_spawn() {
        let local_a = Local::new(0);
        let wg = LocalWaitGroup::new();

        local_scope(|scope| async {
            scope.spawn(async {
                assert_eq!(*local_a.deref(), 2);
                *local_a.get_mut() += 1;
                yield_now().await;
                assert_eq!(*local_a.deref(), 3);
                *local_a.get_mut() += 1;
                wg.done();
            });

            scope.spawn(async {
                assert_eq!(*local_a.deref(), 1);
                *local_a.get_mut() += 1;
                wg.inc();
                wg.wait().await;
                assert_eq!(*local_a.deref(), 4);
                *local_a.get_mut() += 1;
            });

            assert_eq!(*local_a.deref(), 0);
            *local_a.get_mut() += 1;
        })
        .await;

        assert_eq!(*local_a.deref(), 5);

        let local_a = Local::new(0);

        local_scope(|scope| async {
            scope.spawn(async {
                assert_eq!(*local_a.deref(), 1);
                *local_a.get_mut() += 1;
                yield_now().await;
                assert_eq!(*local_a.deref(), 2);
                *local_a.get_mut() += 1;
            });

            scope.spawn(async {
                assert_eq!(*local_a.deref(), 0);
                *local_a.get_mut() += 1;
                sleep(Duration::from_millis(1)).await;
                assert_eq!(*local_a.deref(), 3);
                *local_a.get_mut() += 1;
            });
        })
        .await;

        assert_eq!(*local_a.deref(), 4);
    }
}
