use std::future::Future;
use std::intrinsics::unlikely;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use crate::runtime::local_executor;
use crate::runtime::task::Task;
use crate::sleep::sleeping_task::SleepingTask;

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
            let task = unsafe { (cx.waker().data() as *const Task).read() };
            let mut sleeping_task = SleepingTask::new(this.sleep_until, task);

            while unlikely(!local_executor().sleeping_tasks().insert(sleeping_task)) {
                this.sleep_until += Duration::from_nanos(1);
                sleeping_task = SleepingTask::new(this.sleep_until, task);
            }
            Poll::Pending
        }
    }
}

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
    use std::time::Duration;

    use crate::local::Local;

    use super::*;

    #[orengine_macros::test]
    fn test_sleep() {
        async fn sleep_for(dur: Duration, number: u16, arr: Local<Vec<u16>>) {
            sleep(dur).await;
            arr.get_mut().push(number);
        }

        let arr = Local::new(Vec::new());

        local_executor().exec_future(sleep_for(Duration::from_millis(1), 1, arr.clone()));
        local_executor().exec_future(sleep_for(Duration::from_millis(4), 4, arr.clone()));
        local_executor().exec_future(sleep_for(Duration::from_millis(3), 3, arr.clone()));
        local_executor().exec_future(sleep_for(Duration::from_millis(2), 2, arr.clone()));

        sleep(Duration::from_millis(5)).await;
        assert_eq!(&vec![1, 2, 3, 4], arr.get());

        let arr = Local::new(Vec::new());

        let executor = local_executor();
        executor.spawn_local(sleep_for(Duration::from_millis(1), 1, arr.clone()));
        executor.spawn_local(sleep_for(Duration::from_millis(4), 4, arr.clone()));
        executor.spawn_local(sleep_for(Duration::from_millis(3), 3, arr.clone()));
        executor.spawn_local(sleep_for(Duration::from_millis(2), 2, arr.clone()));

        sleep(Duration::from_millis(5)).await;
        assert_eq!(&vec![1, 2, 3, 4], arr.get());
    }
}
