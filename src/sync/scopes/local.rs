use crate::runtime::{local_executor, Locality};
use crate::sync::{AsyncWaitGroup, LocalWaitGroup};
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
    /// ```rust
    /// use std::ops::Deref;
    /// use std::time::Duration;
    /// use orengine::{sleep, Local};
    /// use orengine::sync::{local_scope, AsyncWaitGroup, LocalWaitGroup};
    ///
    /// # async fn foo() {
    /// let wg = LocalWaitGroup::new();
    /// let a = Local::new(0);
    ///
    /// local_scope(|scope| async {
    ///     for i in 0..10 {
    ///         wg.inc();
    ///         scope.exec(async {
    ///             assert_eq!(*a.borrow(), i);
    ///             *a.borrow_mut() += 1;
    ///             sleep(Duration::from_millis(i)).await;
    ///             wg.done();
    ///         });
    ///     }
    ///
    ///     wg.wait().await;
    ///     assert_eq!(*a.borrow(), 10);
    /// }).await;
    ///
    /// assert_eq!(*a.borrow(), 10);
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

        let local_task = unsafe { crate::runtime::Task::from_future(handle, Locality::local()) };
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
    /// ```rust
    /// use std::ops::Deref;
    /// use orengine::Local;
    /// use orengine::sync::{local_scope, AsyncWaitGroup, LocalWaitGroup};
    ///
    /// # async fn foo() {
    /// let wg = LocalWaitGroup::new();
    /// let a = Local::new(0);
    ///
    /// local_scope(|scope| async {
    ///     for i in 0..10 {
    ///         wg.inc();
    ///         scope.spawn(async {
    ///             *a.borrow_mut() += 1;
    ///             wg.done();
    ///         });
    ///     }
    ///
    ///     assert_eq!(*a.borrow(), 0);
    ///     wg.wait().await;
    ///     assert_eq!(*a.borrow(), 10);
    /// }).await;
    ///
    /// assert_eq!(*a.borrow(), 10);
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

impl<Fut: Future<Output = ()>> Future for LocalScopedHandle<'_, Fut> {
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
/// # The difference between `local_scope` and [`shared_scope`](crate::sync::shared_scope)
///
/// The `local_scope` works with `local tasks`.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// # Example
///
/// ```rust
/// use std::ops::Deref;
/// use std::time::Duration;
/// use orengine::{sleep, Local};
/// use orengine::sync::{local_scope, AsyncWaitGroup, LocalWaitGroup};
///
/// # async fn foo() {
/// let wg = LocalWaitGroup::new();
/// let a = Local::new(0);
///
/// local_scope(|scope| async {
///     for i in 0..10 {
///         wg.inc();
///         scope.exec(async {
///             assert_eq!(*a.borrow(), i);
///             *a.borrow_mut() += 1;
///             sleep(Duration::from_millis(i)).await;
///             wg.done();
///         });
///     }
///
///     wg.wait().await;
///     assert_eq!(*a.borrow(), 10);
/// }).await;
///
/// assert_eq!(*a.borrow(), 10);
/// # }
/// ```
#[inline(always)]
#[allow(clippy::future_not_send, reason = "Because it is `local`")]
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

    static_scope.wg.wait().await;

    yield_now().await; // You can't call 2 local_scopes in the same task if you don't yield
}

/// ```compile_fail
/// use orengine::sync::local_scope;
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// async fn test() {
///     check_send(local_scope(|_| async {})).await;
/// }
/// ```
#[allow(dead_code, reason = "It is used only in compile tests")]
fn test_compile_local_scope() {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::local::Local;
    use crate::yield_now;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::Relaxed;

    #[orengine::test::test_local]
    fn test_local_scope_exec() {
        let local_a = Local::new(0);

        local_scope(|scope| async {
            scope.exec(async {
                assert_eq!(*local_a.borrow(), 0);
                *local_a.borrow_mut() += 1;
                yield_now().await;
                assert_eq!(*local_a.borrow(), 2);
                *local_a.borrow_mut() += 1;
            });

            scope.exec(async {
                assert_eq!(*local_a.borrow(), 1);
                *local_a.borrow_mut() += 1;
                yield_now().await;
                assert_eq!(*local_a.borrow(), 3);
                *local_a.borrow_mut() += 1;
            });
        })
        .await;

        yield_now().await;

        assert_eq!(*local_a.borrow(), 4);
    }

    #[orengine::test::test_local]
    fn test_local_scope_exec_with_main_future() {
        let local_a = Local::new(0);

        local_scope(|scope| async {
            scope.exec(async {
                assert_eq!(*local_a.borrow(), 0);
                *local_a.borrow_mut() += 1;
                yield_now().await;
                assert_eq!(*local_a.borrow(), 3);
                *local_a.borrow_mut() += 1;
            });

            scope.exec(async {
                assert_eq!(*local_a.borrow(), 1);
                *local_a.borrow_mut() += 1;
                yield_now().await;
                assert_eq!(*local_a.borrow(), 4);
                *local_a.borrow_mut() += 1;
            });

            assert_eq!(*local_a.borrow(), 2);
            *local_a.borrow_mut() += 1;
        })
        .await;

        yield_now().await;

        assert_eq!(*local_a.borrow(), 5);
    }

    #[orengine::test::test_local]
    fn test_local_scope_spawn() {
        let local_a = Local::new(0);
        let wg = LocalWaitGroup::new();
        wg.inc();

        local_scope(|scope| async {
            scope.spawn(async {
                assert_eq!(*local_a.borrow(), 1);
                *local_a.borrow_mut() += 1;
                yield_now().await;
                assert_eq!(*local_a.borrow(), 2);
                *local_a.borrow_mut() += 1;
                wg.done();
            });

            scope.spawn(async {
                assert_eq!(*local_a.borrow(), 0);
                *local_a.borrow_mut() += 1;
                wg.wait().await;
                assert_eq!(*local_a.borrow(), 3);
                *local_a.borrow_mut() += 1;
            });
        })
        .await;

        yield_now().await;

        assert_eq!(*local_a.borrow(), 4);
    }

    #[orengine::test::test_local]
    fn test_local_scope_spawn_with_main_future() {
        let local_a = Local::new(0);
        let wg = LocalWaitGroup::new();

        local_scope(|scope| async {
            scope.spawn(async {
                assert_eq!(*local_a.borrow(), 2);
                *local_a.borrow_mut() += 1;
                yield_now().await;
                assert_eq!(*local_a.borrow(), 3);
                *local_a.borrow_mut() += 1;
                wg.done();
            });

            scope.spawn(async {
                assert_eq!(*local_a.borrow(), 1);
                *local_a.borrow_mut() += 1;
                wg.inc();
                wg.wait().await;
                assert_eq!(*local_a.borrow(), 4);
                *local_a.borrow_mut() += 1;
            });

            assert_eq!(*local_a.borrow(), 0);
            *local_a.borrow_mut() += 1;
        })
        .await;

        yield_now().await;

        assert_eq!(*local_a.borrow(), 5);
    }

    #[orengine::test::test_local]
    fn test_many_local_scope_in_the_same_task() {
        static ROUND: AtomicUsize = AtomicUsize::new(0);

        #[allow(clippy::future_not_send, reason = "It is local test")]
        async fn work_with_scope<'scope>(counter: Local<usize>, scope: &LocalScope<'scope>) {
            scope.spawn(async {
                assert_eq!(*counter.borrow(), 2 + 6 * ROUND.load(Relaxed));
                *counter.borrow_mut() += 1;
                yield_now().await;
                assert_eq!(*counter.borrow(), 5 + 6 * ROUND.load(Relaxed));
                *counter.borrow_mut() += 1;
            });

            scope.exec(async {
                assert_eq!(*counter.borrow(), 6 * ROUND.load(Relaxed));
                *counter.borrow_mut() += 1;
                yield_now().await;
                assert_eq!(*counter.borrow(), 3 + 6 * ROUND.load(Relaxed));
                *counter.borrow_mut() += 1;
            });

            assert_eq!(*counter.borrow(), 1 + 6 * ROUND.load(Relaxed));
            *counter.borrow_mut() += 1;
            yield_now().await;
            assert_eq!(*counter.borrow(), 4 + 6 * ROUND.load(Relaxed));
            *counter.borrow_mut() += 1;
        }

        let counter = Local::new(0);

        local_scope(|scope| async {
            work_with_scope(counter.clone(), scope).await;
        })
        .await;

        assert_eq!(*counter.borrow(), 6);
        ROUND.store(1, Relaxed);

        local_scope(|scope| async {
            work_with_scope(counter.clone(), scope).await;
        })
        .await;

        assert_eq!(*counter.borrow(), 12);
        ROUND.store(2, Relaxed);

        for i in 3..10 {
            local_scope(|scope| async {
                work_with_scope(counter.clone(), scope).await;
            })
            .await;

            assert_eq!(*counter.borrow(), i * 6);
            ROUND.store(i, Relaxed);
        }
    }
}
