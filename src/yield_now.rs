use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::{get_task_from_context, local_executor};

/// `Yield` implements the [`Future`] trait for yielding the current task.
///
/// When [`Future::poll`] is called, it will add current task to
/// the beginning of the LIFO queue.
pub struct Yield {
    was_yielded: bool,
}

impl Future for Yield {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = self.get_mut();
        if this.was_yielded {
            Poll::Ready(())
        } else {
            this.was_yielded = true;
            let task = get_task_from_context!(cx);
            if task.is_local() {
                local_executor().add_task_at_the_start_of_lifo_local_queue(task);
                return Poll::Pending;
            }

            local_executor().add_task_at_the_start_of_lifo_shared_queue(task);
            Poll::Pending
        }
    }
}

/// `yield_now` transfers control to the executor and adds the current task
/// to the beginning of the LIFO queue.
///
/// # Example
///
/// ```no_run
/// use std::ops::Deref;
/// use orengine::{yield_now, Local};
///
///  async fn wait(is_ready: Local<bool>) {
///     while !is_ready.deref() {
///         yield_now().await;
///     }
/// }
/// ```
pub fn yield_now() -> Yield {
    Yield { was_yielded: false }
}

#[cfg(test)]
mod tests {
    use crate::local::Local;
    use crate::runtime::local_executor;
    use std::ops::Deref;

    use super::*;
    use crate as orengine;

    #[orengine_macros::test_local]
    fn test_yield_now() {
        let i = Local::new(false);
        let i_clone = i.clone();
        local_executor().spawn_local(async move {
            assert_eq!(*i.deref(), false);
            *i.get_mut() = true;
        });
        yield_now().await;
        assert_eq!(*i_clone.deref(), true);
    }
}
