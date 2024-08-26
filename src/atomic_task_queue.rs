use crossbeam::queue::SegQueue;
use crate::runtime::Task;

pub struct AtomicTaskList {
    inner: SegQueue<Task>
}

impl AtomicTaskList {
    pub const fn new() -> Self {
        Self {
            inner: SegQueue::new()
        }
    }

    /// # Safety
    ///
    /// Called in [`Executor::exec_task`](crate::runtime::Executor::exec_task).
    pub(crate) unsafe fn push(&self, task: Task) {
        self.inner.push(task);
    }

    pub fn pop(&self) -> Option<Task> {
        self.inner.pop()
    }
}