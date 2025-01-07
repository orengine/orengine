use crate::runtime::Task;
use crate::utils::never_wait_lock::NeverWaitLock;
use crate::utils::SpinLockGuard;
use std::collections::VecDeque;
use std::ptr;

/// `SharedExecutorTaskList` is a list of tasks that can be shared between executors.
///
/// All tasks in the list must be `shared` and their futures must implement `Send`.
pub(crate) struct ExecutorSharedTaskList {
    executor_id: usize,
    list: NeverWaitLock<Vec<Task>>,
}

impl ExecutorSharedTaskList {
    /// Creates a new `SharedExecutorTaskList`
    pub(crate) const fn new(executor_id: usize) -> Self {
        Self {
            executor_id,
            list: NeverWaitLock::new(Vec::new()),
        }
    }

    /// Returns the executor id of this `SharedExecutorTaskList`.
    #[inline(always)]
    pub(crate) fn executor_id(&self) -> usize {
        self.executor_id
    }

    /// Returns the `SpinLockGuard<Vec<Task>>` of the underlying list if it is not locked.
    /// Otherwise, returns `None`.
    pub(crate) fn try_lock_and_return_as_vec(&self) -> Option<SpinLockGuard<Vec<Task>>> {
        self.list.try_lock()
    }

    /// Takes at most `limit` tasks from the list and puts them in `other_list`.
    #[inline(always)]
    pub(crate) fn take_batch(&self, other_list: &mut VecDeque<Task>, limit: usize) {
        if let Some(mut guard) = self.list.try_lock() {
            let number_of_elems = guard.len().min(limit);
            let new_len = guard.len() - number_of_elems;
            let mut first_index = new_len;

            while first_index != guard.len() {
                other_list.push_back(unsafe { ptr::read(guard.get_unchecked(first_index)) });
                first_index += 1;
            }
            unsafe { guard.set_len(new_len) };
        }
    }
}

unsafe impl Send for ExecutorSharedTaskList {}
unsafe impl Sync for ExecutorSharedTaskList {}
