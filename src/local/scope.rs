use std::future::Future;
use crate::Executor;
use crate::runtime::{local_executor, Task};

pub struct LocalScope<'scope> {
    executor: &'scope mut Executor
}

impl<'scope: 'static> LocalScope<'scope> {
    pub fn new() -> Self {
        Self {
            executor: local_executor()
        }
    }

    #[inline(always)]
    pub fn exec_future<Fut: Future<Output = ()> + 'scope>(&mut self, f: Fut) {
        self.executor.exec_future(f);
    }

    #[inline(always)]
    pub fn exec_task(&mut self, task: Task) {
        self.executor.exec_task(task);
    }

    #[inline(always)]
    pub fn spawn_local<Fut: Future<Output = ()> + 'scope>(&mut self, f: Fut) {
        self.executor.spawn_local(f);
    }
}