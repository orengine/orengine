use crate::runtime::Task;
use crate::utils::SpinLock;
use std::collections::VecDeque;
use std::ptr;

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

    /// Returns the number of tasks in the list.
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    /// Returns whether the list is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }

    /// Shrinks capacity of the list to `min_capacity`.
    #[inline]
    pub fn shrink_to(&mut self, min_capacity: usize) {
        self.inner.lock().shrink_to(min_capacity);
    }

    /// Pushes a task at the end of the list.
    ///
    /// # Safety
    ///
    /// - Provided task must be `shared`.
    ///
    /// - If called not in [`Future::poll`](std::future::Future::poll).
    ///
    /// In [`Future::poll`](std::future::Future::poll) [`call`](crate::Executor::invoke_call)
    /// [`PushCurrentTaskTo`](crate::runtime::call::Call::PushCurrentTaskTo) instead.
    ///
    /// # Panics
    ///
    /// If provided task is `local` with `debug_assertions`,
    /// else it is an undefined behavior.
    pub unsafe fn push(&self, task: Task) {
        #[cfg(debug_assertions)]
        {
            assert!(!task.is_local());
        }

        self.inner.lock().push(task);
    }

    /// Pops the first task from the list.
    #[inline]
    pub fn pop(&self) -> Option<Task> {
        self.inner.lock().pop()
    }

    /// Pops all tasks from the list and appends them to `tasks`.
    #[inline]
    pub fn pop_all_in(&self, tasks: &mut Vec<Task>) {
        let mut guard = self.inner.lock();
        tasks.append(&mut guard);
    }

    /// Pops all tasks from the list and appends them to provided [`VecDeque`].
    #[inline]
    pub fn pop_all_in_deque(&self, other_list: &mut VecDeque<Task>) {
        let mut guard = self.inner.lock();

        for elem in guard.iter() {
            other_list.push_back(unsafe { ptr::read(elem) });
        }
        unsafe { guard.set_len(0) };
    }
}

impl Default for SyncTaskList {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for SyncTaskList {}
unsafe impl Sync for SyncTaskList {}
