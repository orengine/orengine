use std::cell::UnsafeCell;
use std::future::Future;
use std::intrinsics::unlikely;
use std::pin::Pin;
use std::task::{Context, Poll};
use crate::runtime::local_executor;
use crate::runtime::task::Task;

pub struct Wait<'wait_group> {
    need_wait: bool,
    wait_group: &'wait_group LocalWaitGroup
}

impl<'wait_group> Wait<'wait_group> {
    #[inline(always)]
    fn new(need_wait: bool, wait_group: &'wait_group LocalWaitGroup) -> Self {
        Self {
            need_wait,
            wait_group
        }
    }
}

impl<'wait_group> Future for Wait<'wait_group> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        if !this.need_wait {
            Poll::Ready(())
        } else {
            this.need_wait = false;
            let task = unsafe { (cx.waker().as_raw().data() as *const Task).read() };
            this.wait_group.get_inner().waited_tasks.push(task);
            Poll::Pending
        }
    }
}

struct Inner {
    count: usize,
    waited_tasks: Vec<Task>
}

pub struct LocalWaitGroup {
    inner: UnsafeCell<Inner>
}

impl LocalWaitGroup {
    pub fn new() -> Self {
        Self {
            inner: UnsafeCell::new(Inner { count: 0, waited_tasks: Vec::new() })
        }
    }

    #[inline(always)]
    fn get_inner(&self) -> &mut Inner {
        unsafe { &mut *self.inner.get() }
    }

    #[inline(always)]
    pub fn add(&self, count: usize) {
        self.get_inner().count += count;
    }

    #[inline(always)]
    pub fn inc(&self) {
        self.add(1);
    }

    #[inline(always)]
    pub fn done(&self) {
        let inner = self.get_inner();
        inner.count -= 1;
        if unlikely(inner.count == 0) {
            let executor = local_executor();
            for task in inner.waited_tasks.iter() {
                executor.exec_task(*task);
            }
            unsafe { inner.waited_tasks.set_len(0) };
        }
    }

    #[inline(always)]
    #[must_use="Future must be awaited to start the wait"]
    pub fn wait(&self) -> Wait {
        if unlikely(self.get_inner().count == 0) {
            return Wait::new(false, self);
        }
        Wait::new(true, self)
    }
}

unsafe impl Sync for LocalWaitGroup {}
impl !Send for LocalWaitGroup {}

#[cfg(test)]
mod tests {
    use std::rc::Rc;
    use crate::local::Local;
    use crate::runtime::local_executor;
    use crate::yield_now;
    use super::*;

    #[test_macro::test]
    fn test_many_wait_one() {
        let check_value = Local::new(false);
        let wait_group = Rc::new(LocalWaitGroup::new());
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
    }

    #[test_macro::test]
    fn test_one_wait_many() {
        let check_value = Local::new(10);
        let wait_group = Rc::new(LocalWaitGroup::new());
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

        wait_group.wait().await;
        if *check_value.get() != 0 {
            panic!("not waited");
        }
    }
}