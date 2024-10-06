use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::{local_executor, panic_if_local_in_future};

/// `LocalYield` implements the [`Future`] trait for yielding the current task.
/// 
/// When [`Future::poll`] is called, it will add current task to 
/// the beginning of the `local` LIFO queue.
pub struct LocalYield {
    was_yielded: bool,
}

impl Future for LocalYield {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = self.get_mut();
        if this.was_yielded {
            Poll::Ready(())
        } else {
            this.was_yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// `local_yield_now` transfers control to the executor and adds the current task 
/// to the beginning of the `local` LIFO queue.
/// 
/// # The difference between `local_yield_now` and [`global_yield_now`]
/// 
/// Adds the current task to the beginning of the `local` LIFO queue.
/// For more details, see [`Executor`](crate::runtime::Executor).
/// 
/// # Example
/// 
/// ```no_run
/// use std::ops::Deref;
/// use orengine::{local_yield_now, Local};
///
///  async fn wait(is_ready: Local<bool>) {
///     while !is_ready.deref() {
///         local_yield_now().await;
///     }
/// }
/// ```
pub fn local_yield_now() -> LocalYield {
    LocalYield { was_yielded: false }
}

/// `GlobalYield` implements the [`Future`] trait for yielding the current task.
/// 
/// When [`Future::poll`] is called, it will add current task to 
/// the beginning of the `global` LIFO queue.
pub struct GlobalYield {
    was_yielded: bool,
}

impl Future for GlobalYield {
    type Output = ();

    #[allow(unused)] // because #[cfg(debug_assertions)]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = self.get_mut();
        panic_if_local_in_future!(cx, "global_yield_now()");

        if this.was_yielded {
            Poll::Ready(())
        } else {
            this.was_yielded = true;
            unsafe { local_executor().push_current_task_at_the_start_of_lifo_global_queue() };
            Poll::Pending
        }
    }
}

/// `global_yield_now` transfers control to the executor and adds the current task 
/// to the beginning of the `global` LIFO queue.
/// 
/// # The difference between `global_yield_now` and [`local_yield_now`]
/// 
/// Adds the current task to the beginning of the `global` LIFO queue.
///
/// # Example
///
/// ```no_run
/// use std::sync::atomic::AtomicBool;
/// use std::sync::atomic::Ordering::SeqCst;
/// use orengine::global_yield_now;
///
///  async fn wait(is_ready: &AtomicBool) {
///     while !is_ready.load(SeqCst) {
///         global_yield_now().await;
///     }
/// }
/// ```
pub fn global_yield_now() -> GlobalYield {
    GlobalYield { was_yielded: false }
}

#[cfg(test)]
mod tests {
    use crate::local::Local;
    use crate::runtime::local_executor;
    use std::ops::Deref;

    use super::*;

    #[orengine_macros::test]
    fn test_yield_now() {
        let i = Local::new(false);
        let i_clone = i.clone();
        local_executor().spawn_local(async move {
            assert_eq!(*i.deref(), false);
            *i.get_mut() = true;
        });
        local_yield_now().await;
        assert_eq!(*i_clone.deref(), true);
    }
}
