use crate::runtime::Task;
use crate::utils::{SpinLock, SpinLockGuard};
use std::collections::VecDeque;

/// `SharedExecutorTaskList` is a list of tasks that can be shared between executors.
///
/// All tasks in the list must be `shared` and their futures must implement `Send`.
pub(crate) struct ExecutorSharedTaskList {
    executor_id: usize,
    list: SpinLock<Vec<Task>>,
}

impl ExecutorSharedTaskList {
    /// Creates a new `SharedExecutorTaskList`
    pub(crate) const fn new(executor_id: usize) -> Self {
        Self {
            executor_id,
            list: SpinLock::new(Vec::new()),
        }
    }

    /// Returns the executor id of this `SharedExecutorTaskList`.
    #[inline(always)]
    pub(crate) fn executor_id(&self) -> usize {
        self.executor_id
    }

    /// Returns the `SpinLockGuard<Vec<Task>>` of the underlying list.
    pub(crate) fn as_vec(&self) -> Option<SpinLockGuard<Vec<Task>>> {
        self.list.try_lock()
    }

    /// Takes at most `limit` tasks from the list and puts them in `other_list`.
    #[inline(always)]
    pub(crate) fn take_batch(&self, other_list: &mut VecDeque<Task>, limit: usize) {
        if let Some(mut guard) = self.list.try_lock() {
            let number_of_elems = guard.len().min(limit);
            for elem in guard.drain(..number_of_elems) {
                other_list.push_back(elem);
            }
        }
    }
}

unsafe impl Send for ExecutorSharedTaskList {}
unsafe impl Sync for ExecutorSharedTaskList {}
