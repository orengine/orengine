// TODO docs

use crate::bug_message::BUG_MESSAGE;
use crate::runtime::{Config, Task};
use crate::Executor;
use crossbeam::channel::bounded;
use std::collections::BTreeMap;
use std::future::Future;
use std::thread;

pub struct TestRunner {
    executor_senders: BTreeMap<Config, Task>
}

pub(crate) static WORK_SHARING_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

macro_rules! generate_start_test_body {
    ($cfg:expr, $method:expr, $func:expr, $handle_result_fn:expr) => {{
        let lock = if $cfg.is_work_sharing_enabled() {
            Some(WORK_SHARING_TEST_LOCK.lock().unwrap())
        } else {
            None
        };

        let ex = crate::Executor::init_with_config($cfg);
        let res = $method(ex, $func());
        drop(lock);

        $handle_result_fn(res)
    }};
}

impl TestRunner {
    pub const fn new() -> Self {
        Self {
            executor_senders: BTreeMap::new()
        }
    }

    fn start_executor(&mut self, config: Config) {
        if self.executor_senders.contains_key(&config) {
            panic!("{BUG_MESSAGE}");
        }
        let (sender, receiver) = bounded(0);
        self.executor_senders.insert(config, sender).unwrap();

        thread::spawn(move || {
            let ex = Executor::init_with_config(config);
            ex.run_and_block_on_global(async move {
                loop {
                    let task = receiver.recv().unwrap();
                    ex.exec_task(task);
                }
            }).expect(BUG_MESSAGE);
        });
    }

    fn exec_future(&mut self, task: Task, config: Config, is_local: usize) {
        let lock = if config.is_work_sharing_enabled() {
            Some(WORK_SHARING_TEST_LOCK.lock().unwrap())
        } else {
            None
        };
        
        let ex_sender;
        if let Some(sender) = self.executor_senders.get(&config) {
            ex_sender = sender;
        } else {
            self.start_executor(config);
            ex_sender = self.executor_senders.get(&config).expect(BUG_MESSAGE);
        };

        let (res_sender, res_receiver) = bounded(0);
        ex_sender.send(task).unwrap();

        drop(lock);
    }

    pub fn block_on_local<Ret, Fut, F>(&self, func: F) -> Ret
    where
        Fut: Future<Output = Ret> + 'static,
        F: FnOnce() -> Fut,
    {
        generate_start_test_body!(
            self.cfg,
            Executor::run_and_block_on_local,
            func,
            |res: Result<Ret, &'static str>| -> Ret {
                res.expect("Spawned executor has panicked!")
            }
        )
    }

    pub fn block_on_global<Ret, Fut, F>(&self, func: F) -> Ret
    where
        Fut: Future<Output = Ret> + Send + 'static,
        F: FnOnce() -> Fut,
    {
        generate_start_test_body!(
            self.cfg,
            Executor::run_and_block_on_global,
            func,
            |res: Result<Ret, &'static str>| -> Ret {
                res.expect("Spawned executor has panicked!")
            }
        )
    }
}

pub static DEFAULT_TEST_RUNNER: TestRunner =
    TestRunner::new(Config::default().set_work_sharing_level(usize::MAX));

pub fn run_test_and_block_on_local<Ret, Fut, F>(func: F) -> Ret
where
    Fut: Future<Output = Ret> + 'static,
    F: FnOnce() -> Fut,
{
    DEFAULT_TEST_RUNNER.block_on_local(func)
}

pub fn run_test_and_block_on_global<Ret, Fut, F>(func: F) -> Ret
where
    Fut: Future<Output = Ret> + Send + 'static,
    F: FnOnce() -> Fut,
{
    DEFAULT_TEST_RUNNER.block_on_global(func)
}
