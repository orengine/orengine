use std::collections::BTreeMap;
use std::intrinsics::likely;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Acquire, Release};
use crossbeam::utils::CachePadded;
use crate::local_executor;
use crate::messages::BUG;
use crate::runtime::SharedTaskList;
use crate::utils::{SpinLock, SpinLockGuard};

pub(crate) struct SubscribedState {
    current_version: CachePadded<AtomicUsize>,
    processed_version: usize,
    is_stopped: bool,
    lists: Option<Vec<Arc<SharedTaskList>>>
}

impl SubscribedState {
    #[inline(always)]
    pub(crate) fn check_subscription(&mut self, executor_id: usize) {
        let current_version = self.current_version.load(Acquire);
        if likely(self.processed_version == current_version) {
            // The subscription has valid data
            return;
        }

        self.processed_version = current_version;
        let global_state = global_state();

        if !global_state.alive_executors.contains_key(&executor_id) {
            self.is_stopped = true;
            return;
        }

        if self.lists.is_some() {
            self.lists = Some(global_state.lists.clone());
        }
    }
}

impl SubscribedState {
    pub(crate) const fn new() -> Self {
        Self {
            current_version: CachePadded::new(AtomicUsize::new(1)),
            processed_version: 0,
            is_stopped: false,
            lists: None
        }
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.is_stopped
    }
}

struct GlobalState {
    version: usize,
    /// key is a worker id
    alive_executors: BTreeMap<usize, (Option<Arc<SharedTaskList>>, &'static SubscribedState)>,
    lists: Vec<Arc<SharedTaskList>>
}

impl GlobalState {
    const fn new() -> Self {
        Self {
            version: 0,
            alive_executors: BTreeMap::new(),
            lists: Vec::new()
        }
    }

    fn notify_all(&self) {
        for (_, (_, state)) in self.alive_executors.iter() {
            state.current_version.store(self.version, Release);
        }
    }

    #[inline(always)]
    pub(crate) fn register_local_executor(&mut self) {
        self.version += 1;
        let executor = local_executor();

        executor.subscribed_state_mut().lists = if let Some(shared_task_list) = executor.shared_task_list() {
            let old_lists = Some(self.lists.clone());
            self.lists.push(shared_task_list.clone());

            old_lists
        } else {
            None
        };

        // No need `executor.subscribed_state_mut().current_version.store(self.version, Relaxed)`
        // because notify_all() will set the version
        executor.subscribed_state_mut().processed_version = self.version;

        self.alive_executors.insert(
            executor.executor_id(),
            (executor.shared_task_list().cloned(), executor.subscribed_state())
        );

        self.notify_all();
    }

    #[inline(always)]
    pub(crate) fn stop_executor(&mut self, id: usize) {
        self.version += 1;
        let (
            task_list_,
            subscribed_state
        ) = self.alive_executors.remove(&id).expect(BUG);

        if let Some(task_list) = task_list_ {
            self.lists.retain(|list| !Arc::ptr_eq(list, &task_list));
        }

        subscribed_state.current_version.store(self.version, Release);
        self.notify_all();
    }

    #[inline(always)]
    pub(crate) fn stop_all_executors(&mut self) {
        self.version += 1;
        self.alive_executors.retain(|_, (_, state)| {
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

pub fn stop_executor(executor_id: usize) {
    global_state().stop_executor(executor_id);
}

pub fn stop_all_executors() {
    global_state().stop_all_executors();
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Duration;
    use crate::{local_executor, sleep, Executor};
    use crate::runtime::Config;
    use super::*;

    #[test_macro::test]
    fn test_stop_executor() {
        thread::spawn(move || {
            let ex = Executor::init_with_config(
                Config::default()
                    .disable_work_sharing()
                    .disable_io_worker()
                    .disable_io_worker()
            );
            ex.spawn_local(async  {
                println!("2");
                stop_executor(local_executor().executor_id());
            });
            println!("1");
            ex.run();
            println!("3");
        });

        sleep(Duration::from_millis(100)).await;

        println!("4");
    }
}