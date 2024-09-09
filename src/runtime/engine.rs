use std::{mem, thread};
use std::time::Duration;
use crate::Executor;
use crate::messages::BUG;
use crate::runtime::notification::Notification;
use crate::utils::SpinLock;

struct StopState {
    executor: &'static Executor,
    to_acknowledge: usize
}

struct Engine {
    executors: Vec<&'static Executor>,
    stop_states: Vec<StopState>,
}

impl Engine {
    const fn new() -> Self {
        Self {
            executors: Vec::new(),
            stop_states: Vec::new()
        }
    }

    fn register_executor(
        &mut self,
        executor_ref: &'static Executor
    ) -> Vec<&'static Executor> {
        // Algorithm:
        //
        // 1 - add executor to the executors list. After this all new executors will know about it
        // 2 - notify all executors about new executor. After that all old executors will know about it
        let old_executors = self.executors.clone();
        self.executors.push(executor_ref);

        for executor in self.executors.iter() {
            if *executor as *const Executor == executor_ref as *const Executor {
                continue;
            }

            executor.notify(Notification::RegisteredExecutor(executor_ref));
        }

        old_executors
    }

    fn stop_executor(&mut self, executor_ref: &'static Executor) {
        // Algorithm:
        //
        // 1 - remove the executor from the executors list. After this all new executors will know about it.
        // 2 - push the executor to the stop_states list.
        // 3 - notify all executors about stopping executor. After that all old executors will know about it.
        // 4 - wait to be acknowledged by all old executors.
        // 5 - remove executor from the stop_executors list.

        let mut was_removed = false;
        self.executors.retain(|executor| {
            if *executor as *const Executor != executor_ref as *const Executor {
                true
            } else {
                was_removed = true;
                false
            }
        });
        if !was_removed {
            panic!(
                "Attempt to end an executor that was not registered or was already ended \
                Be careful, because this case may mean that you end not the a right executor. \
                For example, you wanted to end a local executor in the current task, \
                but the current task has been taken by another executor and it call \
                stop_executor(local_executor()). In this case, you must to get executor \
                in the begging of the task."
            );
        }

        if self.executors.len() > 0 {
            self.stop_states.push(StopState {
                executor: executor_ref,
                to_acknowledge: self.executors.len()
            });

            let executor_ptr_usize = executor_ref as *const Executor as usize;
            thread::spawn(move || {
                for _ in 0..100 {
                    thread::sleep(Duration::from_millis(1));
                    let engine = STATIC_ENGINE.lock();
                    let was_stopped = engine.stop_states.iter().find(|stop_state| {
                        stop_state.executor as *const Executor as usize == executor_ptr_usize
                    }).is_none();

                    if was_stopped {
                        return;
                    }
                }

                // some thread panicked
                let mut engine = STATIC_ENGINE.lock();
                engine.stop_states.retain(|stop_state| {
                    if stop_state.executor as *const Executor as usize != executor_ptr_usize {
                        true
                    } else {
                        stop_state.executor.notify(Notification::StoppedCurrentExecutor);
                        false
                    }
                });
            });

            for executor in self.executors.iter() {
                if *executor as *const Executor == executor_ref as *const Executor {
                    panic!("{}", BUG);
                }
                executor.notify(Notification::StoppedExecutor(executor_ref));
            }
        } else {
            executor_ref.notify(Notification::StoppedCurrentExecutor);
        }
    }

    fn acknowledge_stop(&mut self, stop_executor_ref: &'static Executor) {
        for i in 0..self.stop_states.len() {
            let stop_state = &mut self.stop_states[i];
            if stop_state.executor as *const Executor == stop_executor_ref as *const Executor {
                stop_state.to_acknowledge -= 1;

                if stop_state.to_acknowledge == 0 {
                    println!("TODO r: acknowledge_stop");
                    stop_state.executor.notify(Notification::StoppedCurrentExecutor);
                    self.stop_states.remove(i);

                    return;
                }
            }
        }
    }

    fn stop_all_executors(&mut self) {
        for executor in self.executors.iter() {
            executor.notify(Notification::StoppedCurrentExecutor);
        }

        for stop_state in self.stop_states.iter() {
            stop_state.executor.notify(Notification::StoppedCurrentExecutor);
        }
    }
}

unsafe impl Send for Engine {}

static STATIC_ENGINE: SpinLock<Engine> = SpinLock::new(Engine::new());

pub(crate) fn register_executor(executor_ref: &'static Executor) -> Vec<&'static Executor> {
    let mut guard = STATIC_ENGINE.lock();
    println!("TODO r: guard: {}", guard.executors.len());
    guard.register_executor(executor_ref)
}

pub fn stop_executor(executor_ptr: *const Executor) {
    let mut guard = STATIC_ENGINE.lock();
    guard.stop_executor(unsafe { mem::transmute(executor_ptr) });
}

pub(crate) fn acknowledge_stop(executor_ref: &'static Executor) {
    let mut guard = STATIC_ENGINE.lock();
    guard.acknowledge_stop(executor_ref);
}

pub fn stop_all_executors() {
    let mut guard = STATIC_ENGINE.lock();
    guard.stop_all_executors();
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
                stop_executor(local_executor());
            });
            println!("1");
            ex.run();
            println!("3");
        });

        sleep(Duration::from_millis(100)).await;

        println!("4");
    }
}