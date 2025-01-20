use crate::runtime::global_state::subscribed_state::SubscribedState;
#[cfg(not(feature = "disable_send_task_to"))]
use crate::runtime::interaction_between_executors::SyncBatchOptimizedTaskQueue;
use crate::runtime::ExecutorSharedTaskList;
use crate::utils::vec_map::VecMap;
use crate::utils::{SpinLock, SpinLockGuard};
use crate::{local_executor, Executor};
use std::sync::Arc;

/// Contains [`SubscribedState`] and, optionally (`cfg(not(feature = "disable_send_task_to"))`),
/// [`SyncBatchOptimizedTaskQueue`].
pub(crate) struct StateOfAliveExecutor {
    pub(crate) subscribed_state: Arc<SubscribedState>,
    #[cfg(not(feature = "disable_send_task_to"))]
    pub(crate) task_queue: Arc<SyncBatchOptimizedTaskQueue>,
}

impl StateOfAliveExecutor {
    /// Creates a new `StateOfAliveExecutor` of the provided [`Executor`](crate::Executor).
    fn new(executor: &Executor) -> Self {
        Self {
            subscribed_state: executor.subscribed_state(),
            #[cfg(not(feature = "disable_send_task_to"))]
            task_queue: executor.interactor().shared_task_list(),
        }
    }
}

/// `GlobalState` contains current version, `tasks_lists` of all alive executors with work-sharing.
///
/// It also contains `states_of_alive_executors` to update the versions.
///
/// # Note
///
/// [`GLOBAL_STATE`] and [`SubscribedState`] form `Shared RWLock`.
pub struct GlobalState {
    version: usize,
    states_of_alive_executors: VecMap<StateOfAliveExecutor>,
    lists: Vec<Arc<ExecutorSharedTaskList>>,
}

impl GlobalState {
    /// Creates a new `GlobalState`.
    const fn new() -> Self {
        Self {
            version: 0,
            states_of_alive_executors: VecMap::new(),
            lists: Vec::new(),
        }
    }

    /// Registers the executor of the current thread (by calling
    /// [`local_executor()`](local_executor)) and notifies all executors.
    #[inline]
    pub(crate) fn register_local_executor(&mut self) {
        self.version += 1;
        let executor = local_executor();
        if let Some(shared_task_list) = executor.shared_task_list() {
            self.lists.push(shared_task_list.clone());
            executor
                .subscribed_state()
                .set_tasks_lists(Some(self.lists.clone()));

            let executor_id = executor.id();
            executor
                .subscribed_state()
                .validate_tasks_lists(executor_id);
        }

        executor.subscribed_state().set_version(self.version);
        self.states_of_alive_executors
            .insert(executor.id(), StateOfAliveExecutor::new(executor));

        // It is necessary to reset the flag to false on re-initialization.
        executor.subscribed_state().set_is_stopped(false);

        for (_, state) in self.states_of_alive_executors.iter() {
            state.subscribed_state.set_version(self.version);
        }
    }

    /// Stops the executor with the given id and notifies all executors.
    pub(crate) fn stop_executor(&mut self, id: usize) {
        self.version += 1;
        self.states_of_alive_executors
            .retain(|alive_executor_id, state| {
                state.subscribed_state.set_version(self.version);

                alive_executor_id != id
            });
        self.lists.retain(|list| list.executor_id() != id);
    }

    /// Stops all executors.
    pub(crate) fn stop_all_executors(&mut self) {
        self.version += 1;
        self.states_of_alive_executors.retain(|_, state| {
            state.subscribed_state.set_version(self.version);

            false
        });
        self.lists.clear();
    }

    /// Returns a shared reference to all alive executors.
    pub(crate) fn alive_executors(&self) -> &VecMap<StateOfAliveExecutor> {
        &self.states_of_alive_executors
    }

    /// Returns a shared reference to a list of all `shared` task lists.
    pub(crate) fn lists(&self) -> &Vec<Arc<ExecutorSharedTaskList>> {
        &self.lists
    }

    /// Returns ids of all alive executors.
    pub fn executors_ids(&self) -> Vec<usize> {
        self.states_of_alive_executors
            .iter()
            .map(|(id, _)| id)
            .collect()
    }

    /// Returns ids of executors with `work_sharing` enabled.
    pub fn work_sharing_executors_ids(&self) -> Vec<usize> {
        self.states_of_alive_executors
            .iter()
            .filter(|(_, state)| state.subscribed_state.tasks_lists().is_some())
            .map(|(id, _)| id)
            .collect()
    }
}

unsafe impl Send for GlobalState {}

/// `GLOBAL_STATE` contains current version, `tasks_lists` of all alive executors with work-sharing.
/// It and [`SubscribedState`] form `Shared RWLock`.
///
/// Read [`GlobalState`](GlobalState) for more details.
///
/// # Thread safety
///
/// It is thread-safe because it uses [`SpinLock`](SpinLock).
static GLOBAL_STATE: SpinLock<GlobalState> = SpinLock::new(GlobalState::new());

/// Locks `GLOBAL_STATE` and returns [`SpinLockGuard<'static, GlobalState>`](SpinLockGuard).
///
/// Frequent calls to this function may cause performance problems.
///
/// # Example
///
/// ```no_run
/// use orengine::{run_shared_future_on_all_cores, stop_all_executors};
/// use orengine::runtime::lock_and_get_global_state;
/// use orengine::sync::{AsyncWaitGroup, WaitGroup};
/// use orengine::utils::get_core_ids;
///
/// async fn run_shard() {}
///
/// let number_of_cores = get_core_ids().unwrap().len();
/// let wait_group = WaitGroup::new();
///
/// wait_group.add(number_of_cores);
///
/// run_shared_future_on_all_cores(|| async {
///     wait_group.done();
///     wait_group.wait().await;
///
///     assert_eq!(lock_and_get_global_state().executors_ids().len(), number_of_cores);
///
///     // Do some work
///
///     stop_all_executors();
/// });
/// ```
pub fn lock_and_get_global_state() -> SpinLockGuard<'static, GlobalState> {
    GLOBAL_STATE.lock()
}

/// Registers the executor of the current thread (by calling [`local_executor()`](local_executor))
/// and notifies all executors.
pub(crate) fn register_local_executor() {
    lock_and_get_global_state().register_local_executor();
}

/// Stops the executor with the given id.
///
/// # Do not use it in tests!
///
/// Reuse [`Executor`]. Read about it in [`test module`](crate::test).
///
/// # Examples
///
/// ## Correct Usage
///
/// ```no_run
/// use orengine::{Executor, stop_executor, sleep};
/// use std::time::Duration;
///
/// let mut executor = Executor::init();
/// let id = executor.id();
///
/// executor.spawn_local(async move {
///     sleep(Duration::from_secs(3)).await;
///     stop_executor(id); // stops the executor
/// });
/// executor.run();
///
/// println!("Hello from a sync runtime after at least 3 seconds");
/// ```
///
/// ## Incorrect Usage
///
/// You need to save an id when the executor starts because else the task can be moved
/// (if it is shared) to another executor, but it needs to stop the parent executor.
///
/// ```no_run
/// use orengine::{Executor, stop_executor, sleep, local_executor};
/// use std::time::Duration;
///
/// let mut executor = Executor::init();
///
/// executor.spawn_shared(async move {
///     sleep(Duration::from_secs(3)).await;
///     stop_executor(local_executor().id()); // Undefined behavior: stop an unknown executor
/// });
/// executor.run();
///
/// println!("Hello from a sync runtime after at least 3 seconds");
/// ```
pub fn stop_executor(executor_id: usize) {
    lock_and_get_global_state().stop_executor(executor_id);
}

/// Stops all executors.
///
/// # Do not use it in tests!
///
/// Reuse [`Executor`]. Read about it in [`test module`](crate::test).
pub fn stop_all_executors() {
    lock_and_get_global_state().stop_all_executors();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::runtime::Config;
    use crate::{local_executor, sleep, Executor};
    use std::thread;
    use std::time::Duration;

    #[orengine::test::test_local]
    fn test_stop_executor() {
        thread::spawn(move || {
            let ex = Executor::init_with_config(
                Config::default()
                    .disable_work_sharing()
                    .disable_io_worker()
                    .disable_io_worker(),
            );
            ex.spawn_local(async {
                println!("2");
                stop_executor(local_executor().id());
            });
            println!("1");
            ex.run();
            println!("3");
        });

        sleep(Duration::from_millis(100)).await;

        println!("4");
    }
}
