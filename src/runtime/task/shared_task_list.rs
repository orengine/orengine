use std::collections::VecDeque;
use crate::runtime::Task;
use crate::utils::{SpinLock, SpinLockGuard};

/// `SharedExecutorTaskList` is a list of tasks that can be shared between executors.
///
/// All tasks in the list must be `global` and their futures must implement `Send`.
pub(crate) struct SharedExecutorTaskList {
    executor_id: usize,
    list: SpinLock<Vec<Task>>
}

impl SharedExecutorTaskList {
    /// Creates a new `SharedExecutorTaskList`
    pub(crate) const fn new(executor_id: usize) -> Self {
        Self {
            executor_id,
            list: SpinLock::new(Vec::new())
        }
    }

    /// Returns the executor id of this `SharedExecutorTaskList`.
    #[inline(always)]
    pub(crate) fn executor_id(&self) -> usize {
        self.executor_id
    }

    /// Returns whether the list is empty.
    #[inline(always)]
    pub(crate) fn is_empty(&self) -> bool {
        self.list.lock().is_empty()
    }

    /// Returns the `SpinLockGuard<Vec<Task>>` of the underlying list.
    pub(crate) fn as_vec(&self) -> SpinLockGuard<Vec<Task>> {
        self.list.lock()
    }

    /// Takes at most `limit` tasks from the list and puts them in `other_list`.
    #[inline(always)]
    pub(crate) fn take_batch(&self, other_list: &mut VecDeque<Task>, limit: usize) {
        let mut guard = self.list.lock();

        let number_of_elems = guard.len().min(limit);
        for elem in guard.drain(..number_of_elems) {
            other_list.push_back(elem);
        }
    }
}

unsafe impl Send for SharedExecutorTaskList {}
unsafe impl Sync for SharedExecutorTaskList {}