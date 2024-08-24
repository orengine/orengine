use std::future::Future;
use std::intrinsics::unlikely;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Relaxed, Release};
use std::task::{Context, Poll};
use crossbeam::queue::SegQueue;
use crossbeam::utils::Backoff;
use crate::Executor;
use crate::runtime::local_executor;
use crate::runtime::task::Task;

pub struct Wait<'wait_group> {
    wait_group: &'wait_group WaitGroup,
    was_called: bool
}

impl<'wait_group> Wait<'wait_group> {
    #[inline(always)]
    pub(crate) fn new(wait_group: &'wait_group WaitGroup) -> Self {
        Self {
            wait_group,
            was_called: false
        }
    }
}

impl<'wait_group> Future for Wait<'wait_group> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        if !this.was_called {
            this.was_called = true;
            let task = unsafe { (cx.waker().as_raw().data() as *const Task).read() };
            this.wait_group.waited_tasks.push(task);

            // Here I need to explain this decision.
            //
            // I think it's a rare case where all the tasks were completed before the wait was called.
            // Therefore, I sacrifice performance in this case to get much faster in the frequent case.
            //
            // Otherwise, I'd have to keep track of how many tasks are in the queue,
            // which means calling out one more atomic operation in each done call.
            //
            // So I queue the task first, and only then do the check.
            if unlikely(this.wait_group.count.load(Relaxed) == 0) { // all done
                if let Some(task) = this.wait_group.waited_tasks.pop() {
                    unsafe  { local_executor().spawn_local_task(task) };
                }
            }

            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

pub struct WaitGroup {
    count: AtomicUsize,
    waited_tasks: SegQueue<Task>
}

impl WaitGroup {
    pub fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
            waited_tasks: SegQueue::new()
        }
    }

    #[inline(always)]
    pub fn add(&self, count: usize) {
        self.count.fetch_add(count, Relaxed);
    }

    #[inline(always)]
    pub fn inc(&self) {
        self.add(1);
    }

    #[inline(always)]
    pub fn done(&self) {
        let prev = self.count.fetch_sub(1, Release);
        if unlikely(prev == 1) {
            let backoff = Backoff::new();
            loop {
                if let Some(task) = self.waited_tasks.pop() {
                    Executor::exec_task(task);
                    break;
                }
                backoff.spin();
            }
        }
    }

    #[inline(always)]
    #[must_use="Future must be awaited to start the wait"]
    pub fn wait(&self) -> Wait {
        Wait::new(self)
    }
}

unsafe impl Sync for WaitGroup {}
unsafe impl Send for WaitGroup {}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;
    use crate::runtime::create_local_executer_for_block_on;
    use crate::{sleep, yield_now};
    use super::*;

    const PAR: usize = 200;

    #[test_macro::test]
    fn test_many_wait_one() {
        let check_value = Arc::new(Mutex::new(false));
        let wait_group = Arc::new(WaitGroup::new());
        wait_group.inc();

        for _ in 0..PAR {
            let check_value = check_value.clone();
            let wait_group = wait_group.clone();

            thread::spawn(move || {
                create_local_executer_for_block_on(async move {
                    wait_group.wait().await;
                    if !*check_value.lock().unwrap() {
                        panic!("not waited");
                    }
                });
            });
        }

        yield_now().await;

        *check_value.lock().unwrap() = true;
        wait_group.done();
    }

    #[test_macro::test]
    fn test_one_wait_many_task_finished_after_wait() {
        let check_value = Arc::new(Mutex::new(PAR));
        let wait_group = Arc::new(WaitGroup::new());
        wait_group.add(PAR);

        for _ in 0..PAR {
            let check_value = check_value.clone();
            let wait_group = wait_group.clone();

            thread::spawn(move || {
                create_local_executer_for_block_on(async move {
                    sleep(Duration::from_millis(1)).await;
                    *check_value.lock().unwrap() -= 1;
                    wait_group.done();
                });
            });
        }

        wait_group.wait().await;
        if *check_value.lock().unwrap() != 0 {
            panic!("not waited");
        }
    }

    #[test_macro::test]
    fn test_one_wait_many_task_finished_before_wait() {
        let check_value = Arc::new(Mutex::new(PAR));
        let wait_group = Arc::new(WaitGroup::new());
        wait_group.add(PAR);

        for _ in 0..PAR {
            let check_value = check_value.clone();
            let wait_group = wait_group.clone();

            thread::spawn(move || {
                create_local_executer_for_block_on(async move {
                    *check_value.lock().unwrap() -= 1;
                    wait_group.done();
                });
            });
        }

        sleep(Duration::from_millis(1)).await;
        wait_group.wait().await;
        if *check_value.lock().unwrap() != 0 {
            panic!("not waited");
        }
    }
}