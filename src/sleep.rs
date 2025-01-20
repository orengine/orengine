use crate::get_task_from_context;
use crate::runtime::local_executor;
use std::collections::btree_map::Entry::{Occupied, Vacant};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

/// `Sleep` implements the [`Future`] trait. It waits at least until `sleep_until` and works only
/// in `orengine` runtime.
pub struct Sleep {
    was_yielded: bool,
    sleep_until: Instant,
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = &mut *self;
        if this.was_yielded {
            // [`Executor`](crate::Executor) will wake this future up when it should be woken up.
            Poll::Ready(())
        } else {
            this.was_yielded = true;
            let task = unsafe { get_task_from_context!(cx) };

            loop {
                let sleeping_tasks_map = local_executor().sleeping_tasks();
                match sleeping_tasks_map.entry(this.sleep_until) {
                    Occupied(_) => {
                        this.sleep_until += Duration::from_nanos(1);
                    }
                    Vacant(entry) => {
                        entry.insert(task);
                        break;
                    }
                }
            }

            Poll::Pending
        }
    }
}

/// Sleeps at least until `Instant::now() + duration`. It works only in `orengine` runtime.
///
/// # Example
///
/// ```no_run
/// use orengine::sleep;
/// use std::time::Duration;
///
/// orengine::Executor::init().run_with_local_future(async {
///     sleep(Duration::from_millis(100)).await;
///     println!("Hello after at least 100 millis!");
/// });
/// ```
#[inline]
pub fn sleep(duration: Duration) -> Sleep {
    Sleep {
        was_yielded: false,
        sleep_until: local_executor().start_round_time_for_deadlines() + duration,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::local::Local;
    use crate::yield_now;
    use std::time::Duration;

    #[orengine::test::test_local]
    fn test_sleep() {
        #[allow(clippy::future_not_send, reason = "It is `local`.")]
        async fn sleep_for(dur: Duration, number: u16, arr: Local<Vec<u16>>) {
            sleep(dur).await;
            arr.borrow_mut().push(number);
        }

        for _ in 0..100 {
            let arr = Local::new(Vec::new());
            let ex = local_executor();

            yield_now().await; // release exec_series

            ex.exec_local_future(sleep_for(Duration::from_millis(1), 1, arr.clone()));
            ex.exec_local_future(sleep_for(Duration::from_millis(4), 4, arr.clone()));
            ex.exec_local_future(sleep_for(Duration::from_millis(3), 3, arr.clone()));
            ex.exec_local_future(sleep_for(Duration::from_millis(2), 2, arr.clone()));

            sleep(Duration::from_millis(5)).await;
            assert_eq!(&vec![1, 2, 3, 4], &*arr.borrow());
        }
    }
}
