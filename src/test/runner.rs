// TODO docs

use crate::runtime::Config;
use crate::Executor;
use std::future::Future;

pub struct TestRunner {
    cfg: Config,
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
    pub const fn new(cfg: Config) -> Self {
        Self { cfg }
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
