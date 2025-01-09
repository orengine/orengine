use crate::runtime::Task;
use crate::utils::never_wait_lock::NeverWaitLock;
use std::collections::VecDeque;

/// `SyncBatchOptimizedTaskQueue` is a struct that provides a thread-safe ways to appends `local`
/// or `shared` tasks or to take them. It is optimized to work with batches of tasks.
pub(crate) struct SyncBatchOptimizedTaskQueue {
    local_tasks: NeverWaitLock<VecDeque<Task>>,
    shared_tasks: NeverWaitLock<VecDeque<Task>>,
}

impl SyncBatchOptimizedTaskQueue {
    /// Creates a new `SyncBatchOptimizedTaskQueue`.
    pub(crate) const fn new() -> Self {
        Self {
            local_tasks: NeverWaitLock::new(VecDeque::new()),
            shared_tasks: NeverWaitLock::new(VecDeque::new()),
        }
    }

    /// [`Appends`](VecDeque::append) the accumulated tasks to the `local` tasks queue
    /// if it is available.
    ///
    /// Else doesn't do anything.
    ///
    /// Returns `true` if the `local` tasks queue is available or if there are no tasks.
    ///
    /// # Panics
    ///
    /// If there are `shared` task with `debug_assertions`.
    fn try_to_append_local_tasks(
        &self,
        tasks: &mut VecDeque<Task>,
        #[cfg(debug_assertions)] executor_id: usize,
    ) -> bool {
        if tasks.is_empty() {
            return true;
        }

        if let Some(mut guard) = self.local_tasks.try_lock() {
            #[cfg(not(debug_assertions))]
            {
                guard.append(tasks);
            }

            #[cfg(debug_assertions)]
            {
                for mut task in tasks.drain(..) {
                    assert!(task.is_local());

                    task.executor_id = executor_id;

                    guard.push_back(task);
                }
            }

            return true;
        }

        false
    }

    /// [`Appends`](VecDeque::append) the accumulated tasks to the `shared` tasks queue
    /// if it is available.
    ///
    /// Else doesn't do anything.
    ///
    /// Returns `true` if the `shared` tasks queue is available or if there are no tasks.
    ///
    /// # Panics
    ///
    /// If there are `local` task with `debug_assertions`.
    fn try_to_append_shared_tasks(
        &self,
        tasks: &mut VecDeque<Task>,
        #[cfg(debug_assertions)] executor_id: usize,
    ) -> bool {
        if tasks.is_empty() {
            return true;
        }

        if let Some(mut guard) = self.shared_tasks.try_lock() {
            #[cfg(not(debug_assertions))]
            {
                guard.append(tasks);
            }

            #[cfg(debug_assertions)]
            {
                for mut task in tasks.drain(..) {
                    assert!(!task.is_local());

                    task.executor_id = executor_id;

                    guard.push_back(task);
                }
            }

            return true;
        }

        false
    }

    /// Calls [`try_to_append_local_tasks`](Self::try_to_append_local_tasks) and
    /// [`try_to_append_shared_tasks`](Self::try_to_append_shared_tasks).
    ///
    /// Returns `true` if both queues are available or if there are no tasks.
    pub(crate) fn try_to_append_tasks(
        &self,
        local_tasks: &mut VecDeque<Task>,
        shared_tasks: &mut VecDeque<Task>,
        #[cfg(debug_assertions)] executor_id: usize,
    ) -> bool {
        self.try_to_append_local_tasks(local_tasks, executor_id)
            && self.try_to_append_shared_tasks(shared_tasks, executor_id)
    }

    /// Takes all `local` tasks from the `local` tasks queue if it is available.
    ///
    /// Else doesn't do anything.
    ///
    /// Returns if the take operation was success.
    fn try_to_take_batch_of_local_tasks(&self, storage: &mut VecDeque<Task>) -> bool {
        if let Some(mut guard) = self.local_tasks.try_lock() {
            storage.append(&mut guard);

            return true;
        }

        false
    }

    /// Takes all `shared` tasks from the `shared` tasks queue if it is available.
    ///
    /// Else doesn't do anything.
    ///
    /// Returns if the take operation was success.
    fn try_to_take_batch_of_shared_tasks(&self, storage: &mut VecDeque<Task>) -> bool {
        if let Some(mut guard) = self.shared_tasks.try_lock() {
            storage.append(&mut guard);

            return true;
        }

        false
    }

    /// Calls [`take_batch_of_local_tasks`](Self::try_to_take_batch_of_local_tasks) and
    /// [`take_batch_of_shared_tasks`](Self::try_to_take_batch_of_shared_tasks).
    ///
    /// Returns `true` if both queues are available or if there are no tasks.
    pub(crate) fn try_to_take_batch_of_tasks(
        &self,
        local_task_storage: &mut VecDeque<Task>,
        shared_task_storage: &mut VecDeque<Task>,
    ) -> bool {
        self.try_to_take_batch_of_local_tasks(local_task_storage)
            && self.try_to_take_batch_of_shared_tasks(shared_task_storage)
    }
}

unsafe impl Send for SyncBatchOptimizedTaskQueue {}
unsafe impl Sync for SyncBatchOptimizedTaskQueue {}
