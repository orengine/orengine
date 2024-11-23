use crate::bug_message::BUG_MESSAGE;
use crate::local_executor;
use crate::runtime::ExecutorSharedTaskList;
use crate::utils::{SpinLock, SpinLockGuard};
use crossbeam::utils::CachePadded;
use std::cell::UnsafeCell;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::sync::Arc;

/// Inner value of [`SharedState`].
struct Inner {
    current_version: CachePadded<AtomicUsize>,
    processed_version: usize,
    is_stopped: bool,
    tasks_lists: Option<Vec<Arc<ExecutorSharedTaskList>>>,
}

/// `SubscribedState` contains the image of the [`shared state`](GLOBAL_STATE)
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
    /// [`register_local_executor`](SharedState::register_local_executor).
    pub(crate) const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(Inner {
                current_version: CachePadded::new(AtomicUsize::new(1)),
                processed_version: usize::MAX,
                is_stopped: false,
                tasks_lists: None,
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

    /// Removes the list of the executor
    /// and moves all the lists that are after it to the beginning.
    fn validate_tasks_lists(&self, executor_id: usize) {
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

    /// Checks the version of [`shared state`](GLOBAL_STATE) and updates it if needed.
    #[inline(always)]
    pub(crate) fn check_version_and_update_if_needed(&self, executor_id: usize) {
        self.with_inner(|inner| {
            let current_version = inner.current_version.load(Acquire);
            if inner.processed_version == current_version {
                debug_assert_ne!(inner.processed_version, usize::MAX, "{BUG_MESSAGE}");
                return;
            }

            inner.processed_version = current_version;
            let shared_state = shared_state();
            let found = shared_state
                .states_of_alive_executors
                .iter()
                .any(|(alive_executor_id, _)| *alive_executor_id == executor_id);

            if !found {
                inner.is_stopped = true;
                return;
            }

            if inner.tasks_lists.is_some() {
                inner.tasks_lists = Some(shared_state.lists.clone());
                self.validate_tasks_lists(executor_id);
            }
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

/// `SharedState` contains current version, `tasks_lists` of all alive executors with work-sharing.
///
/// It also contains `states_of_alive_executors` to update the versions.
///
/// # Note
///
/// [`GLOBAL_STATE`](GLOBAL_STATE) and [`SubscribedState`] form `Shared RWLock`.
struct SharedState {
    version: usize,
    /// 0 - id
    ///
    /// 1 - state
    states_of_alive_executors: Vec<(usize, Arc<SubscribedState>)>,
    lists: Vec<Arc<ExecutorSharedTaskList>>,
}

impl SharedState {
    /// Creates a new `SharedState`.
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
            executor.subscribed_state().with_inner(|inner| {
                inner.tasks_lists = Some(self.lists.clone());
            });
            let executor_id = executor.id();
            executor
                .subscribed_state()
                .validate_tasks_lists(executor_id);
        }

        executor.subscribed_state().with_inner(|inner| {
            inner.processed_version = self.version;
        });
        self.states_of_alive_executors
            .push((executor.id(), executor.subscribed_state()));

        // It is necessary to reset the flag to false on re-initialization.
        executor.subscribed_state().with_inner(|inner| {
            inner.is_stopped = false;
        });

        for (_, state) in &self.states_of_alive_executors {
            state.with_inner(|inner| {
                inner.current_version.store(self.version, Release);
            });
        }
    }

    /// Stops the executor with the given id and notifies all executors.
    #[inline(always)]
    pub(crate) fn stop_executor(&mut self, id: usize) {
        self.version += 1;
        self.states_of_alive_executors
            .retain(|(alive_executor_id, state)| {
                state.with_inner(|inner| {
                    inner.current_version.store(self.version, Release);
                });
                *alive_executor_id != id
            });
        self.lists.retain(|list| list.executor_id() != id);
    }

    /// Stops all executors.
    #[inline(always)]
    pub(crate) fn stop_all_executors(&mut self) {
        self.version += 1;
        self.states_of_alive_executors.retain(|(_, state)| {
            state.with_inner(|inner| {
                inner.current_version.store(self.version, Release);
            });

            false
        });
        self.lists.clear();
    }
}

unsafe impl Send for SharedState {}

/// `GLOBAL_STATE` contains current version, `tasks_lists` of all alive executors with work-sharing.
/// It and [`SubscribedState`] form `Shared RWLock`.
///
/// Read [`SharedState`](SharedState) for more details.
///
/// # Thread safety
///
/// It is thread-safe because it uses [`SpinLock`](SpinLock).
static GLOBAL_STATE: SpinLock<SharedState> = SpinLock::new(SharedState::new());

/// Locks `GLOBAL_STATE` and returns [`SpinLockGuard<'static, SharedState>`](SpinLockGuard).
fn shared_state() -> SpinLockGuard<'static, SharedState> {
    GLOBAL_STATE.lock()
}

/// Registers the executor of the current thread (by calling [`local_executor()`](local_executor))
/// and notifies all executors.
pub(crate) fn register_local_executor() {
    shared_state().register_local_executor();
}

/// Stops the executor with the given id.
///
/// # Do not use it in tests!
///
/// Reuse [`Executor`](crate::Executor). Read about it in [`test module`](crate::test).
///
/// # Examples
///
/// ## Correct Usage
///
/// ```rust
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
/// ```rust
/// use orengine::{Executor, stop_executor, sleep, local_executor};
/// use std::time::Duration;
///
/// let mut executor = Executor::init();
///
/// executor.spawn_shared(async move {
///     sleep(Duration::from_secs(3)).await;
///     stop_executor(local_executor().id()); // Undefined behavior: stops an unknown executor
/// });
/// executor.run();
///
/// println!("Hello from a sync runtime after at least 3 seconds");
/// ```
pub fn stop_executor(executor_id: usize) {
    shared_state().stop_executor(executor_id);
}

/// Stops all executors.
///
/// # Do not use it in tests!
///
/// Reuse [`Executor`](crate::Executor). Read about it in [`test module`](crate::test).
pub fn stop_all_executors() {
    shared_state().stop_all_executors();
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
