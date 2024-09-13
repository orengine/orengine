use std::future::{Future};
use std::pin::Pin;
use std::task::{Context, Poll};

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
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

pub fn yield_now() -> Yield {
    Yield { was_yielded: false }
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
        yield_now().await;
        assert_eq!(*i_clone.get(), true);
    }
}