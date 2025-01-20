use crate::runtime::Task;
use crate::utils::never_wait_lock::NeverWaitLock;
use std::collections::VecDeque;

/// `LocalAndSharedTaskDeque` stores `local` and `shared` tasks deque.
struct LocalAndSharedTaskDeque {
    local_tasks: VecDeque<Task>,
    shared_tasks: VecDeque<Task>,
}

/// `SyncBatchOptimizedTaskQueue` is a struct that provides a thread-safe ways to appends `local`
/// or `shared` tasks or to take them. It is optimized to work with batches of tasks.
pub(crate) struct SyncBatchOptimizedTaskQueue {
    task_deque: NeverWaitLock<LocalAndSharedTaskDeque>,
}

impl SyncBatchOptimizedTaskQueue {
    /// Creates a new `SyncBatchOptimizedTaskQueue`.
    pub(crate) const fn new() -> Self {
        Self {
            task_deque: NeverWaitLock::new(LocalAndSharedTaskDeque {
                local_tasks: VecDeque::new(),
                shared_tasks: VecDeque::new(),
            }),
        }
    }

    /// [`Appends`](VecDeque::append) the accumulated tasks to the [`LocalAndSharedTaskDeque`]
    /// if it is available.
    ///
    /// Else doesn't do anything.
    ///
    /// Returns `true` if the [`LocalAndSharedTaskDeque`] is available or if there are no tasks.
    ///
    /// # Panics
    ///
    /// If the `shared` task is in `local` task queue with `debug_assertions`.
    pub(crate) fn try_to_append_tasks(
        &self,
        local_tasks: &mut VecDeque<Task>,
        shared_tasks: &mut VecDeque<Task>,
        #[cfg(debug_assertions)] executor_id: usize,
    ) -> bool {
        if local_tasks.is_empty() && shared_tasks.is_empty() {
            return true;
        }

        if let Some(mut guard) = self.task_deque.try_lock() {
            #[cfg(not(debug_assertions))]
            {
                guard.local_tasks.append(local_tasks);
                guard.shared_tasks.append(shared_tasks);
            }

            #[cfg(debug_assertions)]
            {
                for mut task in local_tasks.drain(..) {
                    assert!(task.is_local());

                    task.executor_id = executor_id;

                    guard.local_tasks.push_back(task);
                }

                for task in shared_tasks.drain(..) {
                    assert!(!task.is_local());

                    guard.shared_tasks.push_back(task);
                }
            }

            return true;
        }

        false
    }

    /// Takes all tasks to the `local` and `shared` task queues if [`LocalAndSharedTaskDeque`]
    /// is available.
    ///
    /// Else doesn't do anything.
    ///
    /// Returns whether the take operation was success.
    pub(crate) fn try_to_take_batch_of_tasks(
        &self,
        local_tasks: &mut VecDeque<Task>,
        shared_tasks: &mut VecDeque<Task>,
    ) -> bool {
        if let Some(mut guard) = self.task_deque.try_lock() {
            if !guard.local_tasks.is_empty() {
                local_tasks.append(&mut guard.local_tasks);
            }

            if !guard.shared_tasks.is_empty() {
                shared_tasks.append(&mut guard.shared_tasks);
            }

            return true;
        }

        false
    }
}

unsafe impl Send for SyncBatchOptimizedTaskQueue {}
unsafe impl Sync for SyncBatchOptimizedTaskQueue {}
