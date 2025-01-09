use crate::runtime::interaction_between_executors::interactor::{
    Interactor, SharedTaskListForSendTo,
};
use crate::runtime::interaction_between_executors::SyncBatchOptimizedTaskQueue;
use crate::runtime::{lock_and_get_global_state, ExecutorSharedTaskList};
use crate::utils::vec_map::VecMap;
use crate::BUG_MESSAGE;
use crossbeam::utils::CachePadded;
use std::cell::UnsafeCell;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::sync::Arc;

/// Inner value of [`GlobalState`](crate::runtime::global_state::GlobalState).
struct Inner {
    current_version: CachePadded<AtomicUsize>,
    processed_version: usize,
    is_stopped: bool,
    tasks_lists: Option<Vec<Arc<ExecutorSharedTaskList>>>,
    /// Used in `send_task_to`.
    executors_task_lists: VecMap<Arc<SyncBatchOptimizedTaskQueue>>,
}

/// `SubscribedState` contains the image of the [`shared state`](crate::runtime::global_state::global_state::GLOBAL_STATE)
/// and the version. Before use the image, it checks the version and updates it if needed.
///
/// It allows to avoid lock contention, because almost always it can read the image without locking.
///
/// It contains `is_stopped` and `tasks_lists` of all alive executors with work-sharing.
pub(crate) struct SubscribedState {
    inner: UnsafeCell<Inner>,
}

impl SubscribedState {
    /// Creates a new `SubscribedState`.
    ///
    /// It **must** be registered by
    /// [`register_local_executor`](crate::runtime::global_state::GlobalState::register_local_executor).
    pub(crate) const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(Inner {
                current_version: CachePadded::new(AtomicUsize::new(1)),
                processed_version: usize::MAX,
                is_stopped: false,
                tasks_lists: None,
                executors_task_lists: VecMap::new(),
            }),
        }
    }

    /// Runs the closure with a mutable reference to the [`inner`](Inner) value.
    fn with_inner<R, F: FnOnce(&mut Inner) -> R>(&self, f: F) -> R {
        unsafe { f(&mut *self.inner.get()) }
    }

    /// Returns whether current executor is stopped.
    pub(crate) fn is_stopped(&self) -> bool {
        self.with_inner(|inner| inner.is_stopped)
    }

    /// Sets whether current executor is stopped.
    pub(crate) fn set_is_stopped(&self, is_stopped: bool) {
        self.with_inner(|inner| inner.is_stopped = is_stopped);
    }

    /// Sets the version of [`shared state`](crate::runtime::global_state::global_state::GLOBAL_STATE).
    pub(crate) fn set_version(&self, version: usize) {
        self.with_inner(|inner| inner.current_version.store(version, Release));
    }

    /// Returns a shared reference to a list of all `shared` task lists.
    pub(crate) fn tasks_lists(&self) -> Option<&Vec<Arc<ExecutorSharedTaskList>>> {
        unsafe { &*self.inner.get() }.tasks_lists.as_ref()
    }

    /// Sets a list of all `shared` task lists.
    pub(crate) fn set_tasks_lists(&self, tasks_lists: Option<Vec<Arc<ExecutorSharedTaskList>>>) {
        self.with_inner(|inner| inner.tasks_lists = tasks_lists);
    }

    /// Removes the list of the executor
    /// and moves all the lists that are after it to the beginning.
    pub(crate) fn validate_tasks_lists(&self, executor_id: usize) {
        self.with_inner(|inner| {
            if inner.tasks_lists.is_none() {
                return;
            }

            let tasks_lists = inner.tasks_lists.as_ref().unwrap();
            let index = tasks_lists
                .iter()
                .position(|list| list.executor_id() == executor_id)
                .unwrap();

            let len = inner.tasks_lists.as_ref().unwrap().len() - 1;
            if len == 0 {
                inner.tasks_lists = Some(vec![]);
                return;
            }

            let mut new_list = tasks_lists.clone();
            new_list.remove(index);
            new_list.rotate_left(index);

            inner.tasks_lists = Some(new_list);
        });
    }

    /// Compares the version with [`global state`] and updates it if needed.
    ///
    /// [`global state`]: crate::runtime::global_state::global_state::GLOBAL_STATE
    #[inline(always)]
    pub(crate) fn check_version_and_update_if_needed(
        &self,
        executor_id: usize,
        interactor: &mut Interactor,
    ) {
        self.with_inner(|inner| {
            let current_version = inner.current_version.load(Acquire);
            if inner.processed_version == current_version {
                debug_assert_ne!(inner.processed_version, usize::MAX, "{BUG_MESSAGE}");
                return;
            }

            inner.processed_version = current_version;
            let shared_state = lock_and_get_global_state();
            let found = shared_state
                .alive_executors()
                .iter()
                .any(|(alive_executor_id, _)| alive_executor_id == executor_id);

            if !found {
                inner.is_stopped = true;
                return;
            }

            if inner.tasks_lists.is_some() {
                inner.tasks_lists = Some(shared_state.lists().clone());
                self.validate_tasks_lists(executor_id);
            }

            // region update executors_task_lists and interactor.other_shared_task_lists

            inner.executors_task_lists.clear();

            shared_state
                .alive_executors()
                .iter()
                .for_each(|(id, (_, task_queue))| {
                    inner.executors_task_lists.insert(id, task_queue.clone());
                });

            interactor.shared_task_lists_mut().clear();

            shared_state
                .alive_executors()
                .iter()
                .for_each(|(id, (_, task_queue))| {
                    interactor
                        .shared_task_lists_mut()
                        .insert(id, SharedTaskListForSendTo::new(task_queue.clone()));
                });

            // endregion
        });
    }

    /// Returns `tasks_lists` of all alive executors with work-sharing.
    ///
    /// # Safety
    ///
    /// Tasks list must be not None.
    pub(crate) unsafe fn with_tasks_lists<F>(&self, f: F)
    where
        F: FnOnce(&Vec<Arc<ExecutorSharedTaskList>>),
    {
        self.with_inner(|inner| {
            f(unsafe { inner.tasks_lists.as_ref().unwrap_unchecked() });
        });
    }
}

unsafe impl Sync for SubscribedState {}
