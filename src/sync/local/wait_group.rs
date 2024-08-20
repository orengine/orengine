use std::future::Future;
use std::intrinsics::unlikely;
use std::pin::Pin;
use std::task::{Context, Poll};
use crate::Executor;
use crate::local::Local;
use crate::runtime::task::Task;

pub struct Wait {
    need_wait: bool,
    wait_group: LocalWaitGroup
}

impl Future for Wait {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        if !this.need_wait {
            Poll::Ready(())
        } else {
            this.need_wait = false;
            let task = unsafe { (cx.waker().as_raw().data() as *const Task).read() };
            this.wait_group.inner.get_mut().waited_tasks.push(task);
            Poll::Pending
        }
    }
}

struct Inner {
    count: usize,
    waited_tasks: Vec<Task>
}

pub struct LocalWaitGroup {
    inner: Local<Inner>
}

impl LocalWaitGroup {
    pub fn new() -> Self {
        Self {
            inner: Local::new(Inner { count: 0, waited_tasks: Vec::new() })
        }
    }

    #[inline(always)]
    pub fn add(&self, count: usize) {
        self.inner.get_mut().count += count;
    }

    #[inline(always)]
    pub fn inc(&self) {
        self.add(1);
    }

    #[inline(always)]
    pub fn done(&self) {
        let inner = self.inner.get_mut();
        inner.count -= 1;
        if unlikely(inner.count == 0) {
            for task in inner.waited_tasks.iter() {
                Executor::exec_task(*task);
            }
            unsafe { inner.waited_tasks.set_len(0) };
        }
    }

    #[inline(always)]
    #[must_use="Future must be awaited to start the wait"]
    pub fn wait(&self) -> Wait {
        if unlikely(self.inner.get().count == 0) {
            return Wait { need_wait: false, wait_group: self.clone() };
        }
        Wait { need_wait: true, wait_group: self.clone() }
    }
}

impl Clone for LocalWaitGroup {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

unsafe impl Sync for LocalWaitGroup {}

#[cfg(test)]
mod tests {
    use crate::runtime::{create_local_executer_for_block_on, local_executor};
    use crate::yield_now;
    use super::*;

    #[test]
    fn test_many_wait_one() {
        create_local_executer_for_block_on(async {
            let check_value = Local::new(false);
            let wait_group = LocalWaitGroup::new();
            wait_group.inc();

            for _ in 0..10 {
                let check_value = check_value.clone();
                let wait_group = wait_group.clone();
                local_executor().spawn_local(async move {
                    wait_group.wait().await;
                    if !check_value.get() {
                        panic!("not waited");
                    }
                });
            }

            yield_now().await;

            *check_value.get_mut() = true;
            wait_group.done();
        });
    }

    #[test]
    fn test_one_wait_many() {
        create_local_executer_for_block_on(async {
            let check_value = Local::new(10);
            let wait_group = LocalWaitGroup::new();
            wait_group.add(10);

            for _ in 0..10 {
                let check_value = check_value.clone();
                let wait_group = wait_group.clone();
                local_executor().spawn_local(async move {
                    yield_now().await;
                    *check_value.get_mut() -= 1;
                    wait_group.done();
                });
            }

            yield_now().await;

            wait_group.wait().await;
            if *check_value.get() != 0 {
                panic!("not waited");
            }
        });
    }
}