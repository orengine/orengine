use std::collections::VecDeque;
use crate::runtime::Task;
use crate::utils::SpinLock;

pub struct AtomicTaskList {
    inner: SpinLock<Vec<Task>>
}

impl AtomicTaskList {
    pub const fn new() -> Self {
        Self {
            inner: SpinLock::new(Vec::new()),
        }
    }

    /// # Safety
    ///
    /// If called not in [`Future::poll`](std::future::Future::poll).
    pub unsafe fn push(&self, task: Task) {
        self.inner.lock().push(task);
    }

    pub fn pop(&self) -> Option<Task> {
        self.inner.lock().pop()
    }

    #[inline(always)]
    pub(crate) fn take_batch(&self, other_list: &mut VecDeque<Task>, limit: usize) {
        let mut guard = self.inner.lock();

        let number_of_elems = guard.len().min(limit);
        for elem in guard.drain(..number_of_elems) {
            other_list.push_back(elem);
        }
    }
}

unsafe impl Send for AtomicTaskList {}
unsafe impl Sync for AtomicTaskList {}