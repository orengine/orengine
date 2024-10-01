use crate::runtime::local_executor;
use std::future::Future;
use std::pin::Pin;
use std::task::Poll;
use crate::panic_if_local_in_future;
use crate::sync::WaitGroup;

pub struct Scope<'scope> {
    wg: WaitGroup,
    _scope: std::marker::PhantomData<&'scope ()>,
}

impl<'scope> Scope<'scope> {
    #[inline(always)]
    pub fn exec<F: Future<Output = ()> + Send>(&'scope self, future: F) {
        self.wg.inc();
        let handle = ScopedHandle {
            scope: self,
            fut: future,
        };
        
        #[cfg(debug_assertions)]
        {
            let mut global_task = crate::runtime::Task::from_future(handle);
            global_task.is_local = false;
            local_executor().exec_task(global_task);
        }
        
        #[cfg(not(debug_assertions))]
        { local_executor().exec_future(handle); }
    }

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

pub struct ScopedHandle<'scope, Fut: Future<Output = ()> + Send> {
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
    use crate::{local_yield_now, sleep};
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
                local_yield_now().await;
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
                local_yield_now().await;
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
