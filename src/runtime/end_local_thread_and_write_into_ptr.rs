use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use crate::{local_executor, stop_executor};

// Async block in async block allocates double memory.
// But if we use async block in `Future::poll`, it allocates only one memory.
// When I say "async block" I mean future that is represented by `async {}`.
pub(crate) struct EndLocalThreadAndWriteIntoPtr<R, Fut: Future<Output = R>> {
    res_ptr: *mut Option<R>,
    future: Fut,
    local_executor_id: usize
}

impl <R, Fut: Future<Output = R>> EndLocalThreadAndWriteIntoPtr<R, Fut> {
    pub(crate) fn new(res_ptr: *mut Option<R>, future: Fut) -> Self {
        Self {
            res_ptr,
            future,
            local_executor_id: local_executor().id()
        }
    }
}

impl<R, Fut: Future<Output = R>> Future for EndLocalThreadAndWriteIntoPtr<R, Fut> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let mut pinned_fut = unsafe { Pin::new_unchecked(&mut this.future) };
        match pinned_fut.as_mut().poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(res) => {
                unsafe { this.res_ptr.write(Some(res)) };
                stop_executor(this.local_executor_id);
                Poll::Ready(())
            }
        }
    }
}

unsafe impl<R, Fut: Future<Output = R> + Send> Send for EndLocalThreadAndWriteIntoPtr<R, Fut> {}