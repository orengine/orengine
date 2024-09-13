use std::future::Future;
use std::pin::Pin;
use std::task::Poll;
use crate::runtime::local_executor;
use crate::sync::LocalWaitGroup;

pub struct LocalScope<'scope> {
    wg: LocalWaitGroup,
    _scope: std::marker::PhantomData<&'scope ()>
}

impl<'scope> LocalScope<'scope> {
    #[inline(always)]
    pub fn spawn<F: Future<Output=()>>(&'scope self, future: F) {
        self.wg.inc();
        let handle = ScopedHandle {
            scope: self,
            fut: future
        };

        local_executor().exec_future(handle);
    }
}

pub struct ScopedHandle<'scope, Fut: Future<Output=()>> {
    scope: &'scope LocalScope<'scope>,
    fut: Fut
}

impl<'scope, Fut: Future<Output=()>> Future for ScopedHandle<'scope, Fut> {
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

#[inline(always)]
pub async fn local_scope<'scope, Fut, F>(f: F)
where
    Fut: Future<Output=()>,
    F: FnOnce(&'scope LocalScope<'scope>) -> Fut
{
    let scope = LocalScope {
        wg: LocalWaitGroup::new(),
        _scope: std::marker::PhantomData
    };
    let static_scope = unsafe {
        std::mem::transmute::<&_, &'static LocalScope<'static>>(&scope)
    };

    f(static_scope).await;

    scope.wg.wait().await;
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use crate::local::Local;
    use crate::{sleep, yield_now};
    use super::*;

    #[orengine_macros::test]
    fn test_scope() {
        let local_a = Local::new(0);

        local_scope(|scope| async {
            scope.spawn(async {
                assert_eq!(*local_a.get(), 0);
                *local_a.get_mut() += 1;
                yield_now().await;
                assert_eq!(*local_a.get(), 4);
                *local_a.get_mut() += 1;
            });

            scope.spawn(async {
                assert_eq!(*local_a.get(), 1);
                *local_a.get_mut() += 1;
                sleep(Duration::from_millis(1)).await;
                assert_eq!(*local_a.get(), 6);
                *local_a.get_mut() += 1;
            });

            // Do not call await here
            scope.spawn(async {
                assert_eq!(*local_a.get(), 2);
                *local_a.get_mut() += 1;
                yield_now().await;
                assert_eq!(*local_a.get(),5);
                *local_a.get_mut() += 1;
            });

            assert_eq!(*local_a.get(), 3);
            *local_a.get_mut() += 1;
        }).await;

        assert_eq!(*local_a.get(), 7);
    }
}