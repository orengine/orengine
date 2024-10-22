use crate::get_task_from_context;
use crate::runtime::local_executor;
use crate::sleep::sleeping_task::SleepingTask;
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

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        if this.was_yielded {
            // [`Executor::background`] will wake this future up when it should be woken up.
            Poll::Ready(())
        } else {
            this.was_yielded = true;
            let task = get_task_from_context!(cx);
            let mut sleeping_task = SleepingTask::new(this.sleep_until, task);

            while !local_executor()
                .sleeping_tasks()
                .insert(sleeping_task.clone())
            {
                sleeping_task.increment_time_to_wake();
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
/// fn main() {
///     orengine::Executor::init().run_with_local_future(async {
///         sleep(Duration::from_millis(100)).await;
///         println!("Hello after at least 100 millis!");
///     });
/// }
/// ```
#[inline(always)]
#[must_use]
pub fn sleep(duration: Duration) -> Sleep {
    Sleep {
        was_yielded: false,
        sleep_until: Instant::now() + duration,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::local::Local;
    use std::ops::Deref;
    use std::time::Duration;

    #[orengine_macros::test_local]
    fn test_sleep() {
        async fn sleep_for(dur: Duration, number: u16, arr: Local<Vec<u16>>) {
            sleep(dur).await;
            arr.get_mut().push(number);
        }

        let arr = Local::new(Vec::new());
        let ex = local_executor();

        ex.exec_local_future(sleep_for(Duration::from_millis(1), 1, arr.clone()));
        ex.exec_local_future(sleep_for(Duration::from_millis(4), 4, arr.clone()));
        ex.exec_local_future(sleep_for(Duration::from_millis(3), 3, arr.clone()));
        ex.exec_local_future(sleep_for(Duration::from_millis(2), 2, arr.clone()));

        sleep(Duration::from_millis(5)).await;
        assert_eq!(&vec![1, 2, 3, 4], arr.deref());

        let arr = Local::new(Vec::new());

        ex.spawn_local(sleep_for(Duration::from_millis(1), 1, arr.clone()));
        ex.spawn_local(sleep_for(Duration::from_millis(4), 4, arr.clone()));
        ex.spawn_local(sleep_for(Duration::from_millis(3), 3, arr.clone()));
        ex.spawn_local(sleep_for(Duration::from_millis(2), 2, arr.clone()));

        sleep(Duration::from_millis(5)).await;
        assert_eq!(&vec![1, 2, 3, 4], arr.deref());
    }
}
