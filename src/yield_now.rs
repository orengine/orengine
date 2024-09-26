use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::{local_executor, panic_if_local_in_future};

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

pub fn local_yield_now() -> LocalYield {
    LocalYield { was_yielded: false }
}

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

pub fn global_yield_now() -> GlobalYield {
    GlobalYield { was_yielded: false }
}

#[cfg(test)]
mod tests {
    use crate::local::Local;
    use crate::runtime::local_executor;

    use super::*;

    #[orengine_macros::test]
    fn test_yield_now() {
        let i = Local::new(false);
        let i_clone = i.clone();
        local_executor().spawn_local(async move {
            assert_eq!(*i.get(), false);
            *i.get_mut() = true;
        });
        local_yield_now().await;
        assert_eq!(*i_clone.get(), true);
    }
}
