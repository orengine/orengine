use std::collections::BTreeMap;
use std::intrinsics::likely;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Acquire, Release};
use crossbeam::utils::CachePadded;
use crate::Executor;
use crate::messages::BUG;
use crate::runtime::SharedTaskList;
use crate::utils::SpinLock;

pub(crate) struct SubscribedState {
    version: usize,
    is_stopped: bool,
    lists: Option<Vec<Arc<SharedTaskList>>>
}

impl SubscribedState {
    pub(crate) const fn new() -> Self {
        Self {
            version: 0,
            is_stopped: false,
            lists: None
        }
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.is_stopped
    }
}

struct GlobalState {
    version: CachePadded<AtomicUsize>,
    /// key is a worker id
    alive_executors: SpinLock<BTreeMap<usize, Option<Arc<SharedTaskList>>>>,
    lists: SpinLock<Vec<Arc<SharedTaskList>>>
}

impl GlobalState {
    const fn new() -> Self {
        Self {
            version: CachePadded::new(AtomicUsize::new(1)),
            alive_executors: SpinLock::new(BTreeMap::new()),
            lists: SpinLock::new(Vec::new())
        }
    }

    #[inline(always)]
    pub(crate) fn register_executor(
        &self,
        executor: &mut Executor
    ) {
        let mut alive_executors = self.alive_executors.lock();
        alive_executors.insert(executor.executor_id(), executor.shared_task_list().cloned());
        alive_executors.unlock();

        let mut subscribed_state = SubscribedState {
            version: 0,
            is_stopped: false,
            lists: None
        };

        if let Some(shared_task_list) = executor.shared_task_list() {
            let mut lists = self.lists.lock();
            let old_lists = lists.clone();
            lists.push(shared_task_list.clone());
            lists.unlock();

            subscribed_state.lists = Some(old_lists);
        }

        subscribed_state.version = self.version.fetch_add(1, Release) + 1;

        executor.set_subscribed_state(subscribed_state)
    }

    #[inline(always)]
    pub(crate) fn stop_executor(&self, id: usize) {
        let mut alive_executors = self.alive_executors.lock();
        let task_list_ = alive_executors.remove(&id).expect(BUG);
        alive_executors.unlock();

        if let Some(task_list) = task_list_ {
            let mut lists = self.lists.lock();
            lists.retain(|list| !Arc::ptr_eq(list, &task_list));
            lists.unlock();
        }

        self.version.fetch_add(1, Release);
    }

    #[inline(always)]
    pub(crate) fn stop_all_executors(&self) {
        let mut alive_executors = self.alive_executors.lock();
        alive_executors.clear();
        alive_executors.unlock();

        let mut lists = self.lists.lock();
        lists.clear();
        lists.unlock();

        self.version.fetch_add(1, Release);
    }

    #[inline(always)]
    pub(crate) fn check_subscription(&self, executor_id: usize, subscribed_state: &mut SubscribedState) {
        let current_version = self.version.load(Acquire);
        if likely(subscribed_state.version == current_version) {
            // The subscription has valid data
            return;
        }

        subscribed_state.version = current_version;

        let executors = self.alive_executors.lock();
        if !executors.contains_key(&executor_id) {
            subscribed_state.is_stopped = true;
            return;
        }

        executors.unlock();

         if subscribed_state.lists.is_some() {
             subscribed_state.lists = Some(self.lists.lock().clone());
         }
    }
}

static GLOBAL_STATE: GlobalState = GlobalState::new();

pub(crate) fn register_executor(executor_ref: &mut Executor) {
    GLOBAL_STATE.register_executor(executor_ref)
}

pub(crate) fn check_subscription(executor_id: usize, subscribed_state: &mut SubscribedState) {
    GLOBAL_STATE.check_subscription(executor_id, subscribed_state);
}

pub fn stop_executor(executor_id: usize) {
    GLOBAL_STATE.stop_executor(executor_id);
}

pub fn stop_all_executors() {
    GLOBAL_STATE.stop_all_executors();
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Duration;
    use crate::{local_executor, sleep};
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