use crate::bug_message::BUG_MESSAGE;
use crate::local_executor;
use crate::runtime::ExecutorSharedTaskList;
use crate::utils::{SpinLock, SpinLockGuard};
use crossbeam::utils::CachePadded;
use std::cell::UnsafeCell;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::sync::Arc;

/// `SubscribedState` contains the image of the [`global state`](GLOBAL_STATE)
/// and the version. Before use the image, it checks the version and updates it if needed.
///
/// It allows to avoid lock contention, because almost always it can read the image without locking.
///
/// It contains `is_stopped` and `tasks_lists` of all alive executors with work-sharing.
pub(crate) struct SubscribedState {
    current_version: CachePadded<AtomicUsize>,
    processed_version: usize,
    is_stopped: bool,
    tasks_lists: Option<Vec<Arc<ExecutorSharedTaskList>>>,
}

impl SubscribedState {
    /// Creates a new `SubscribedState`.
    ///
    /// It **must** be registered by
    /// [`register_local_executor`](GlobalState::register_local_executor).
    pub(crate) const fn new() -> Self {
        Self {
            current_version: CachePadded::new(AtomicUsize::new(1)),
            processed_version: usize::MAX,
            is_stopped: false,
            tasks_lists: None,
        }
    }

    /// Returns whether current executor is stopped.
    pub(crate) fn is_stopped(&self) -> bool {
        self.is_stopped
    }

    /// Removes the list of the executor
    /// and moves all the lists that are after it to the beginning.
    fn validate_tasks_lists(&mut self, executor_id: usize) {
        if self.tasks_lists.is_none() {
            return;
        }

        let tasks_lists = self.tasks_lists.as_ref().unwrap();
        let index = tasks_lists
            .iter()
            .position(|list| list.executor_id() == executor_id)
            .unwrap();

        let len = self.tasks_lists.as_ref().unwrap().len() - 1;
        if len == 0 {
            self.tasks_lists = Some(vec![]);
            return;
        }

        let mut new_list = tasks_lists.clone();
        new_list.remove(index);
        new_list.rotate_left(index);

        self.tasks_lists = Some(new_list);
    }

    /// Checks the version of [`global state`](GLOBAL_STATE) and updates it if needed.
    #[inline(always)]
    pub(crate) fn check_version_and_update_if_needed(&mut self, executor_id: usize) {
        let current_version = self.current_version.load(Acquire);
        if self.processed_version == current_version {
            debug_assert_ne!(self.processed_version, usize::MAX, "{BUG_MESSAGE}");
            return;
        }

        self.processed_version = current_version;
        let global_state = global_state();
        let found = global_state
            .states_of_alive_executors
            .iter()
            .position(|(alive_executor_id, _)| *alive_executor_id == executor_id)
            .is_some();

        if !found {
            self.is_stopped = true;
            return;
        }

        if self.tasks_lists.is_some() {
            self.tasks_lists = Some(global_state.lists.clone());
            self.validate_tasks_lists(executor_id);
        }
    }

    /// Returns `tasks_lists` of all alive executors with work-sharing.
    ///
    /// # Safety
    ///
    /// Tasks list must be not None.
    pub(crate) unsafe fn tasks_lists(&self) -> &Vec<Arc<ExecutorSharedTaskList>> {
        unsafe { self.tasks_lists.as_ref().unwrap_unchecked() }
    }
}

/// `GlobalState` contains current version, `tasks_lists` of all alive executors with work-sharing.
///
/// It also contains `states_of_alive_executors` to update the versions.
///
/// # Note
///
/// [`GLOBAL_STATE`](GLOBAL_STATE) and [`SubscribedState`] form `Shared RWLock`.
struct GlobalState {
    version: usize,
    /// 0 - id
    ///
    /// 1 - state
    states_of_alive_executors: Vec<(usize, Arc<UnsafeCell<SubscribedState>>)>,
    lists: Vec<Arc<ExecutorSharedTaskList>>,
}

impl GlobalState {
    /// Creates a new `GlobalState`.
    const fn new() -> Self {
        Self {
            version: 0,
            states_of_alive_executors: Vec::new(),
            lists: Vec::new(),
        }
    }

    /// Registers the executor of the current thread (by calling
    /// [`local_executor()`](local_executor)) and notifies all executors.
    #[inline(always)]
    pub(crate) fn register_local_executor(&mut self) {
        self.version += 1;
        let executor = local_executor();
        if let Some(shared_task_list) = executor.shared_task_list() {
            self.lists.push(shared_task_list.clone());
            executor.subscribed_state_mut().tasks_lists = Some(self.lists.clone());
            let executor_id = executor.id();
            executor
                .subscribed_state_mut()
                .validate_tasks_lists(executor_id);
        }

        executor.subscribed_state_mut().processed_version = self.version;
        self.states_of_alive_executors
            .push((executor.id(), executor.subscribed_state()));

        // It is necessary to reset the flag to false on re-initialization.
        executor.subscribed_state_mut().is_stopped = false;

        for (_, state) in &mut self.states_of_alive_executors {
            unsafe { &(&*state.get()).current_version }.store(self.version, Release);
        }
    }

    /// Stops the executor with the given id and notifies all executors.
    #[inline(always)]
    pub(crate) fn stop_executor(&mut self, id: usize) {
        self.version += 1;
        self.states_of_alive_executors
            .retain(|(alive_executor_id, state)| {
                unsafe { &(&*state.get()).current_version }.store(self.version, Release);
                *alive_executor_id != id
            });
        self.lists.retain(|list| list.executor_id() != id);
    }

    /// Stops all executors.
    #[inline(always)]
    pub(crate) fn stop_all_executors(&mut self) {
        self.version += 1;
        self.states_of_alive_executors.retain(|(_, state)| {
            unsafe { &(&*state.get()).current_version }.store(self.version, Release);
            false
        });
        self.lists.clear();
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
fn global_state() -> SpinLockGuard<'static, GlobalState> {
    GLOBAL_STATE.lock()
}

/// Registers the executor of the current thread (by calling [`local_executor()`](local_executor))
/// and notifies all executors.
pub(crate) fn register_local_executor() {
    global_state().register_local_executor()
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
/// fn main() {
///     let mut executor = Executor::init();
///     let id = executor.id();
///
///     executor.spawn_local(async move {
///         sleep(Duration::from_secs(3)).await;
///         stop_executor(id); // stops the executor
///     });
///     executor.run();
///
///     println!("Hello from a sync runtime after at least 3 seconds");
/// }
/// ```
///
/// ## Incorrect Usage
///
/// You need to save an id when the executor starts because else the task can be moved
/// (if it is global) to another executor, but it needs to stop the parent executor.
///
/// ```no_run
/// use orengine::{Executor, stop_executor, sleep, local_executor};
/// use std::time::Duration;
///
/// fn main() {
///     let mut executor = Executor::init();
///
///     executor.spawn_global(async move {
///         sleep(Duration::from_secs(3)).await;
///         stop_executor(local_executor().id()); // Undefined behavior: stops an unknown executor
///     });
///     executor.run();
///
///     println!("Hello from a sync runtime after at least 3 seconds");
/// }
/// ```
pub fn stop_executor(executor_id: usize) {
    global_state().stop_executor(executor_id);
}

/// Stops all executors.
///
/// # Do not use it in tests!
///
/// Reuse [`Executor`]. Read about it in [`test module`](crate::test).
pub fn stop_all_executors() {
    global_state().stop_all_executors();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::runtime::Config;
    use crate::{local_executor, sleep, Executor};
    use std::thread;
    use std::time::Duration;

    #[orengine_macros::test_local]
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
