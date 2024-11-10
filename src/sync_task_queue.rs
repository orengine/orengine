use crate::runtime::Task;
use crate::utils::SpinLock;
use std::collections::VecDeque;

/// `SyncTaskList` is a list of tasks that can be shared between threads.
///
/// # Usage
///
/// Use it only for creating your own [`futures`](std::future::Future) to safe `shared` tasks
/// in these futures.
pub struct SyncTaskList {
    inner: SpinLock<Vec<Task>>,
}

impl SyncTaskList {
    /// Create a new `SyncTaskList`.
    pub const fn new() -> Self {
        Self {
            inner: SpinLock::new(Vec::new()),
        }
    }

    /// Pushes a task at the end of the list.
    ///
    /// # Safety
    ///
    /// If called not in [`Future::poll`](std::future::Future::poll).
    ///
    /// In [`Future::poll`](std::future::Future::poll) use
    /// [`Executor::push_current_task_to`](crate::runtime::Executor::push_current_task_to) instead.
    pub unsafe fn push(&self, task: Task) {
        self.inner.lock().push(task);
    }

    /// Pops the first task from the list.
    #[inline(always)]
    pub fn pop(&self) -> Option<Task> {
        self.inner.lock().pop()
    }

    /// Pops all tasks from the list and appends them to `tasks`.
    #[inline(always)]
    pub fn pop_all_in(&self, tasks: &mut Vec<Task>) {
        let mut guard = self.inner.lock();
        tasks.append(&mut guard);
    }

    /// Takes a batch of tasks from the list. It never takes more than `limit`.
    #[inline(always)]
    pub(crate) fn take_batch(&self, other_list: &mut VecDeque<Task>, limit: usize) {
        let mut guard = self.inner.lock();

        let number_of_elems = guard.len().min(limit);
        for elem in guard.drain(..number_of_elems) {
            other_list.push_back(elem);
        }
    }
}

unsafe impl Send for SyncTaskList {}
unsafe impl Sync for SyncTaskList {}
