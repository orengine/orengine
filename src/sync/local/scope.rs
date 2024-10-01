use crate::runtime::local_executor;
use crate::sync::LocalWaitGroup;
use std::future::Future;
use std::pin::Pin;
use std::task::Poll;

pub struct LocalScope<'scope> {
    wg: LocalWaitGroup,
    _scope: std::marker::PhantomData<&'scope ()>,
}

impl<'scope> LocalScope<'scope> {
    #[inline(always)]
    pub fn exec<F: Future<Output = ()>>(&'scope self, future: F) {
        self.wg.inc();
        let handle = LocalScopedHandle {
            scope: self,
            fut: future,
        };

        local_executor().exec_future(handle);
    }

    #[inline(always)]
    pub fn spawn<F: Future<Output = ()>>(&'scope self, future: F) {
        self.wg.inc();
        let handle = LocalScopedHandle {
            scope: self,
            fut: future,
        };

        local_executor().spawn_local(handle);
    }
}

impl !Send for LocalScope<'_> {}

pub struct LocalScopedHandle<'scope, Fut: Future<Output = ()>> {
    scope: &'scope LocalScope<'scope>,
    fut: Fut,
}

impl<'scope, Fut: Future<Output = ()>> Future for LocalScopedHandle<'scope, Fut> {
    type Output = ();

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

impl<F: Future<Output = ()>> !Send for LocalScopedHandle<'_, F> {}

#[inline(always)]
pub async fn local_scope<'scope, Fut, F>(f: F)
where
    Fut: Future<Output = ()>,
    F: FnOnce(&'scope LocalScope<'scope>) -> Fut,
{
    let scope = LocalScope {
        wg: LocalWaitGroup::new(),
        _scope: std::marker::PhantomData,
    };
    let static_scope = unsafe { std::mem::transmute::<&_, &'static LocalScope<'static>>(&scope) };

    f(static_scope).await;

    scope.wg.wait().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::Local;
    use crate::{local_yield_now, sleep};
    use std::ops::Deref;
    use std::time::Duration;

    #[orengine_macros::test]
    fn test_scope_exec() {
        let local_a = Local::new(0);

        local_scope(|scope| async {
            scope.exec(async {
                assert_eq!(*local_a.deref(), 0);
                *local_a.get_mut() += 1;
                local_yield_now().await;
                assert_eq!(*local_a.deref(), 3);
                *local_a.get_mut() += 1;
            });

            scope.exec(async {
                assert_eq!(*local_a.deref(), 1);
                *local_a.get_mut() += 1;
                sleep(Duration::from_millis(1)).await;
                assert_eq!(*local_a.deref(), 4);
                *local_a.get_mut() += 1;
            });

            assert_eq!(*local_a.deref(), 2);
            *local_a.get_mut() += 1;
        })
        .await;

        assert_eq!(*local_a.deref(), 5);
    }

    #[orengine_macros::test]
    fn test_scope_spawn() {
        let local_a = Local::new(0);

        local_scope(|scope| async {
            scope.spawn(async {
                assert_eq!(*local_a.deref(), 2);
                *local_a.get_mut() += 1;
                local_yield_now().await;
                assert_eq!(*local_a.deref(), 3);
                *local_a.get_mut() += 1;
            });

            scope.spawn(async {
                assert_eq!(*local_a.deref(), 1);
                *local_a.get_mut() += 1;
                sleep(Duration::from_millis(1)).await;
                assert_eq!(*local_a.deref(), 4);
                *local_a.get_mut() += 1;
            });

            assert_eq!(*local_a.deref(), 0);
            *local_a.get_mut() += 1;
        })
            .await;

        assert_eq!(*local_a.deref(), 5);
    }
}
