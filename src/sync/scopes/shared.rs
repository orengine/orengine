use crate::runtime::{local_executor, Locality};
use crate::sync::{AsyncWaitGroup, WaitGroup};
use crate::{panic_if_local_in_future, yield_now};
use std::future::Future;
use std::pin::Pin;
use std::task::Poll;

/// A scope to spawn scoped shared tasks in.
///
/// # The difference between `Scope` and [`LocalScope`](crate::sync::LocalScope)
///
/// The `Scope` works with `shared tasks` and its tasks can be shared between threads.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// See [`shared_scope`] for details.
pub struct Scope<'scope> {
    wg: WaitGroup,
    _scope: std::marker::PhantomData<&'scope ()>,
}

impl<'scope> Scope<'scope> {
    /// Executes a new shared task within a scope.
    ///
    /// Unlike non-scoped tasks, tasks created with this function may
    /// borrow non-`'static` data from the outside the scope. See [`shared_scope`] for
    /// details.
    ///
    /// The created task will be executed immediately.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    /// use std::time::Duration;
    /// use orengine::sleep;
    /// use orengine::sync::{shared_scope, AsyncWaitGroup, WaitGroup};
    ///
    /// # async fn foo() {
    /// let wg = WaitGroup::new();
    /// let a = AtomicUsize::new(0);
    ///
    /// shared_scope(|scope| async {
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

        let shared_task = crate::runtime::Task::from_future(handle, Locality::shared());
        local_executor().exec_task(shared_task);
    }

    /// Spawns a new shared task within a scope.
    ///
    /// Unlike non-scoped tasks, tasks created with this function may
    /// borrow non-`'static` data from the outside the scope. See [`shared_scope`] for
    /// details.
    ///
    /// The created task will be executed later.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    /// use orengine::sync::{shared_scope, AsyncWaitGroup, WaitGroup};
    ///
    /// # async fn foo() {
    /// let wg = WaitGroup::new();
    /// let a = AtomicUsize::new(0);
    ///
    /// shared_scope(|scope| async {
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

        local_executor().spawn_shared(handle);
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

/// Creates a [`shared scope`] for spawning scoped shared tasks.
///
/// The function passed to `scope` will be provided a [`Scope`] object,
/// through which scoped shared tasks can be [spawned][`Scope::spawn`]
/// or [executed][`Scope::exec`].
///
/// Unlike non-scoped tasks, scoped tasks can borrow non-`'static` data,
/// as the scope guarantees all tasks will be awaited at the end of the scope.
///
/// # The difference between `shared_scope` and [`local_scope`](crate::sync::local_scope)
///
/// The `shared_scope` works with `shared tasks` and its tasks can be shared between threads.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// # Example
///
/// ```rust
/// use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
/// use std::time::Duration;
/// use orengine::sleep;
/// use orengine::sync::{shared_scope, AsyncWaitGroup, WaitGroup};
///
/// # async fn foo() {
/// let wg = WaitGroup::new();
/// let a = AtomicUsize::new(0);
///
/// shared_scope(|scope| async {
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
#[allow(
    clippy::future_not_send,
    reason = "It is not `Send` only when F is not `Send`, it is fine"
)]
pub async fn shared_scope<'scope, Fut, F>(f: F)
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

    scope.wg.wait().await;

    yield_now().await; // You can't call 2 shared_scopes in the same task if you don't yield
}

/// ```rust
/// use orengine::sync::shared_scope;
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// async fn test() {
///     check_send(shared_scope(|_| async {})).await;
/// }
/// ```
#[allow(dead_code, reason = "It is used only in compile tests")]
fn test_compile_shared_scope() {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::yield_now;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::{Relaxed, SeqCst};

    #[orengine::test::test_shared]
    fn test_shared_scope_exec() {
        let a = AtomicUsize::new(0);
        let wg = WaitGroup::new();

        shared_scope(|scope| async {
            scope.exec(async {
                assert_eq!(a.load(SeqCst), 0);
                a.fetch_add(1, SeqCst);
                yield_now().await;
                assert_eq!(a.load(SeqCst), 2);
                a.fetch_add(1, SeqCst);
                wg.done();
            });

            scope.exec(async {
                assert_eq!(a.load(SeqCst), 1);
                a.fetch_add(1, SeqCst);
                wg.inc();
                wg.wait().await;
                assert_eq!(a.load(SeqCst), 3);
                a.fetch_add(1, SeqCst);
            });
        })
        .await;

        yield_now().await;

        assert_eq!(a.load(SeqCst), 4);
    }

    #[orengine::test::test_shared]
    fn test_shared_scope_exec_with_main_future() {
        let a = AtomicUsize::new(0);
        let wg = WaitGroup::new();

        shared_scope(|scope| async {
            scope.exec(async {
                assert_eq!(a.load(SeqCst), 0);
                a.fetch_add(1, SeqCst);
                yield_now().await;
                assert_eq!(a.load(SeqCst), 3);
                a.fetch_add(1, SeqCst);
                wg.done();
            });

            scope.exec(async {
                assert_eq!(a.load(SeqCst), 1);
                a.fetch_add(1, SeqCst);
                wg.inc();
                wg.wait().await;
                assert_eq!(a.load(SeqCst), 4);
                a.fetch_add(1, SeqCst);
            });

            assert_eq!(a.load(SeqCst), 2);
            a.fetch_add(1, SeqCst);
        })
        .await;

        yield_now().await;

        assert_eq!(a.load(SeqCst), 5);
    }

    #[orengine::test::test_shared]
    fn test_shared_scope_spawn() {
        let a = AtomicUsize::new(0);
        let wg = WaitGroup::new();
        wg.inc();

        shared_scope(|scope| async {
            scope.spawn(async {
                assert_eq!(a.load(SeqCst), 1);
                a.fetch_add(1, SeqCst);
                yield_now().await;
                assert_eq!(a.load(SeqCst), 2);
                a.fetch_add(1, SeqCst);
                wg.done();
            });

            scope.spawn(async {
                assert_eq!(a.load(SeqCst), 0);
                a.fetch_add(1, SeqCst);
                wg.wait().await;
                assert_eq!(a.load(SeqCst), 3);
                a.fetch_add(1, SeqCst);
            });
        })
        .await;

        yield_now().await;

        assert_eq!(a.load(SeqCst), 4);
    }

    #[orengine::test::test_shared]
    fn test_shared_scope_spawn_with_main_future() {
        let a = AtomicUsize::new(0);
        let wg = WaitGroup::new();

        shared_scope(|scope| async {
            wg.add(1);
            scope.spawn(async {
                assert_eq!(a.load(SeqCst), 2);
                a.fetch_add(1, SeqCst);
                yield_now().await;
                assert_eq!(a.load(SeqCst), 3);
                a.fetch_add(1, SeqCst);
                wg.done();
            });

            scope.spawn(async {
                assert_eq!(a.load(SeqCst), 1);
                a.fetch_add(1, SeqCst);
                wg.wait().await;
                assert_eq!(a.load(SeqCst), 4);
                a.fetch_add(1, SeqCst);
            });

            assert_eq!(a.load(SeqCst), 0);
            a.fetch_add(1, SeqCst);
        })
        .await;

        yield_now().await;

        assert_eq!(a.load(SeqCst), 5);
    }

    #[orengine::test::test_shared]
    fn test_many_shared_scope_in_the_same_task() {
        static ROUND: AtomicUsize = AtomicUsize::new(0);

        #[allow(clippy::future_not_send, reason = "It is local test")]
        async fn work_with_scope<'scope>(counter: &'scope AtomicUsize, scope: &Scope<'scope>) {
            scope.spawn(async {
                assert_eq!(counter.load(Relaxed), 2 + 6 * ROUND.load(Relaxed));
                counter.fetch_add(1, Relaxed);
                yield_now().await;
                assert_eq!(counter.load(Relaxed), 5 + 6 * ROUND.load(Relaxed));
                counter.fetch_add(1, Relaxed);
            });

            scope.exec(async {
                assert_eq!(counter.load(Relaxed), 6 * ROUND.load(Relaxed));
                counter.fetch_add(1, Relaxed);
                yield_now().await;
                assert_eq!(counter.load(Relaxed), 3 + 6 * ROUND.load(Relaxed));
                counter.fetch_add(1, Relaxed);
            });

            assert_eq!(counter.load(Relaxed), 1 + 6 * ROUND.load(Relaxed));
            counter.fetch_add(1, Relaxed);
            yield_now().await;
            assert_eq!(counter.load(Relaxed), 4 + 6 * ROUND.load(Relaxed));
            counter.fetch_add(1, Relaxed);
        }

        let counter = AtomicUsize::new(0);

        shared_scope(|scope| async {
            work_with_scope(&counter, scope).await;
        })
        .await;

        assert_eq!(counter.load(Relaxed), 6);
        ROUND.store(1, Relaxed);

        shared_scope(|scope| async {
            work_with_scope(&counter, scope).await;
        })
        .await;

        assert_eq!(counter.load(Relaxed), 12);
        ROUND.store(2, Relaxed);

        for i in 3..10 {
            shared_scope(|scope| async {
                work_with_scope(&counter, scope).await;
            })
            .await;

            assert_eq!(counter.load(Relaxed), i * 6);
            ROUND.store(i, Relaxed);
        }
    }
}
