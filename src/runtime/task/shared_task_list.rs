use std::collections::VecDeque;
use crate::runtime::Task;
use crate::utils::{SpinLock, SpinLockGuard};

pub(crate) struct SharedExecutorTaskList {
    executor_id: usize,
    list: SpinLock<Vec<Task>>
}

impl SharedExecutorTaskList {
    pub(crate) const fn new(executor_id: usize) -> Self {
        Self {
            executor_id,
            list: SpinLock::new(Vec::new())
        }
    }

    #[inline(always)]
    pub(crate) fn executor_id(&self) -> usize {
        self.executor_id
    }

    #[inline(always)]
    pub(crate) fn is_empty(&self) -> bool {
        self.list.lock().is_empty()
    }

    pub(crate) fn as_vec(&self) -> SpinLockGuard<Vec<Task>> {
        self.list.lock()
    }

    #[inline(always)]
    pub(crate) fn push(&self, task: Task) {
        self.list.lock().push(task);
    }

    #[inline(always)]
    /// # Safety
    ///
    /// other_list must have at least `limit` reserved capacity
    pub(crate) unsafe fn take_batch(&self, other_list: &mut VecDeque<Task>, limit: usize) {
        assert!(other_list.len() + limit <= other_list.capacity());
        let mut guard = self.list.lock();

        let number_of_elems = guard.len().min(limit);
        for elem in guard.drain(..number_of_elems) {
            other_list.push_back(elem);
        }
    }
}

unsafe impl Send for SharedExecutorTaskList {}
unsafe impl Sync for SharedExecutorTaskList {}