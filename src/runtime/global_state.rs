// TODO docs
use crate::local_executor;
use crate::runtime::ExecutorSharedTaskList;
#[cfg(test)]
use crate::test::is_executor_id_in_pool;
use crate::utils::{SpinLock, SpinLockGuard};
use crossbeam::utils::CachePadded;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::sync::Arc;

pub(crate) struct SubscribedState {
    current_version: CachePadded<AtomicUsize>,
    processed_version: usize,
    is_stopped: bool,
    tasks_lists: Option<Vec<Arc<ExecutorSharedTaskList>>>,
}

impl SubscribedState {
    #[inline(always)]
    pub(crate) fn check_subscription(&mut self, executor_id: usize) {
        let current_version = self.current_version.load(Acquire);
        if self.processed_version == current_version {
            // The subscription has valid data
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
}

impl SubscribedState {
    pub(crate) const fn new() -> Self {
        Self {
            current_version: CachePadded::new(AtomicUsize::new(1)),
            processed_version: 0,
            is_stopped: false,
            tasks_lists: None,
        }
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.is_stopped
    }

    /// # Safety
    ///
    /// Tasks list must be not None
    pub(crate) unsafe fn tasks_lists(&self) -> &Vec<Arc<ExecutorSharedTaskList>> {
        unsafe { self.tasks_lists.as_ref().unwrap_unchecked() }
    }
}

struct GlobalState {
    version: usize,
    /// 0 - id
    ///
    /// 1 - state
    states_of_alive_executors: Vec<(usize, &'static SubscribedState)>,
    lists: Vec<Arc<ExecutorSharedTaskList>>,
}

impl GlobalState {
    const fn new() -> Self {
        Self {
            version: 0,
            states_of_alive_executors: Vec::new(),
            lists: Vec::new(),
        }
    }

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

        for (_, state) in &mut self.states_of_alive_executors {
            state.current_version.store(self.version, Release);
        }
    }

    #[inline(always)]
    pub(crate) fn stop_executor(&mut self, id: usize) {
        self.version += 1;
        self.states_of_alive_executors
            .retain(|(alive_executor_id, state)| {
                state.current_version.store(self.version, Release);
                *alive_executor_id != id
            });
        self.lists.retain(|list| list.executor_id() != id);
    }

    #[inline(always)]
    #[cfg(not(test))]
    pub(crate) fn stop_all_executors(&mut self) {
        self.version += 1;
        self.states_of_alive_executors.retain(|(_, state)| {
            state.current_version.store(self.version, Release);
            false
        });
        self.lists.clear();
    }
}

static GLOBAL_STATE: SpinLock<GlobalState> = SpinLock::new(GlobalState::new());

fn global_state() -> SpinLockGuard<'static, GlobalState> {
    GLOBAL_STATE.lock()
}

pub(crate) fn register_local_executor() {
    global_state().register_local_executor()
}

/// Stops the executor with the given id.
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
    #[cfg(test)]
    {
        if is_executor_id_in_pool(executor_id) {
            return;
        }
    }

    global_state().stop_executor(executor_id);
}

/// Stops all executors after some time (at most 100ms).
pub fn stop_all_executors() {
    if cfg!(test) {
        panic!("stop_all_executors is not supported in test mode");
    }

    #[cfg(not(test))]
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
