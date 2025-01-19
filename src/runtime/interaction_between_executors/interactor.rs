use crate::runtime::interaction_between_executors::{SendTaskResult, SyncBatchOptimizedTaskQueue};
use crate::runtime::Task;
use crate::utils::vec_map::VecMap;
use std::collections::VecDeque;
use std::mem;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// `OtherSharedTaskList` contains the list and `local` and `shared` task batches.
pub(crate) struct SharedTaskListForSendTo {
    shared_task_list: Arc<SyncBatchOptimizedTaskQueue>,
    local_task_batch: VecDeque<Task>,
    shared_task_batch: VecDeque<Task>,
}

impl SharedTaskListForSendTo {
    /// Creates a new `OtherSharedTaskList`.
    pub(crate) fn new(shared_task_list: Arc<SyncBatchOptimizedTaskQueue>) -> Self {
        Self {
            shared_task_list,
            local_task_batch: VecDeque::new(),
            shared_task_batch: VecDeque::new(),
        }
    }
}

/// `Interactor` isolates the logic of interacting with other executors.
pub(crate) struct Interactor {
    shared_task_list: Arc<SyncBatchOptimizedTaskQueue>,
    all: VecMap<SharedTaskListForSendTo>,
    id_to_retry: Vec<usize>,
    last_executed_time: Instant,
}

impl Interactor {
    /// Creates a new `Interactor`.
    pub(crate) fn new() -> Self {
        Self {
            shared_task_list: Arc::new(SyncBatchOptimizedTaskQueue::new()),
            all: VecMap::new(),
            id_to_retry: Vec::new(),
            last_executed_time: unsafe { mem::zeroed() },
        }
    }

    /// Returns a shared reference to the `shared` task list of the current executor.
    pub(crate) fn shared_task_list(&self) -> Arc<SyncBatchOptimizedTaskQueue> {
        self.shared_task_list.clone()
    }

    /// Returns a mutable reference to the `shared` task lists of executors.
    pub(crate) fn shared_task_lists_mut(&mut self) -> &mut VecMap<SharedTaskListForSendTo> {
        &mut self.all
    }

    /// Sends a [`Task`] to the executor with the given id.
    pub(crate) fn send_task_to_executor(
        &mut self,
        task: Task,
        executor_id: usize,
    ) -> SendTaskResult {
        if let Some(shared_task_list) = self.all.get_mut(executor_id) {
            if task.is_local() {
                shared_task_list.local_task_batch.push_back(task);
            } else {
                shared_task_list.shared_task_batch.push_back(task);
            }

            return SendTaskResult::Ok;
        }

        SendTaskResult::ExecutorIsNotRegistered
    }

    /// Flushes the accumulated tasks to the other executors.
    ///
    /// It also writes ids of executors that was unable to receive tasks to `id_to_retry`.
    fn flush(&mut self) {
        debug_assert!(self.id_to_retry.is_empty());

        for (id, shared_task_list_) in self.all.iter_mut() {
            let shared_task_list = &shared_task_list_.shared_task_list;

            #[cfg(not(debug_assertions))]
            {
                if !shared_task_list.try_to_append_tasks(
                    &mut shared_task_list_.local_task_batch,
                    &mut shared_task_list_.shared_task_batch,
                ) {
                    self.id_to_retry.push(id);
                }
            }

            #[cfg(debug_assertions)]
            {
                if !shared_task_list.try_to_append_tasks(
                    &mut shared_task_list_.local_task_batch,
                    &mut shared_task_list_.shared_task_batch,
                    id,
                ) {
                    self.id_to_retry.push(id);
                }
            }
        }
    }

    /// Retries flushing the accumulated tasks to the other executors from `id_to_retry`.
    ///
    /// It doesn't guarantee that the tasks will be flushed, but the chance is very high.
    fn retry_flush(&mut self) {
        for id in self.id_to_retry.drain(..) {
            let shared_task_list = unsafe { self.all.get_mut(id).unwrap_unchecked() };

            #[cfg(not(debug_assertions))]
            {
                shared_task_list.shared_task_list.try_to_append_tasks(
                    &mut shared_task_list.local_task_batch,
                    &mut shared_task_list.shared_task_batch,
                );
            }

            #[cfg(debug_assertions)]
            {
                shared_task_list.shared_task_list.try_to_append_tasks(
                    &mut shared_task_list.local_task_batch,
                    &mut shared_task_list.shared_task_batch,
                    id,
                );
            }
        }
    }

    /// Tries to take task from the current executor's `shared_task_list`.
    ///
    /// Returns `true` if the tasks was taken from `local` __and__ `shared` task queues.
    fn try_to_take_tasks(
        &self,
        local_tasks: &mut VecDeque<Task>,
        shared_tasks: &mut VecDeque<Task>,
    ) -> bool {
        self.shared_task_list
            .try_to_take_batch_of_tasks(local_tasks, shared_tasks)
    }

    /// Flushes the accumulated tasks to the other executors and takes task
    /// from the current executor's `shared_task_list`.
    pub(crate) fn do_work(
        &mut self,
        local_tasks: &mut VecDeque<Task>,
        shared_tasks: &mut VecDeque<Task>,
        now: Instant,
    ) {
        if now - self.last_executed_time < Duration::from_micros(64) {
            // We need to limit the number of times this method is called in a short period of time.
            // Otherwise, Executors would try to acquire NeverWaitLock too often and starve each other.
            return;
        }

        self.last_executed_time = now;

        let is_success = self.try_to_take_tasks(local_tasks, shared_tasks);
        self.flush();

        if !is_success {
            self.try_to_take_tasks(local_tasks, shared_tasks);
        }

        self.retry_flush();
    }
}
