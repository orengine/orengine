// TODO docs

use crate::runtime::Config;
use crate::test::ExecutorThreadSpawner;
use crate::Executor;
use std::future::Future;

pub struct TestRunner {
    cfg: Config,
}

macro_rules! generate_start_test_body {
    ($cfg:expr, $method:expr, $func:expr, $handle_result_fn:expr) => {{
        let mut spawner = crate::test::executor_thread_spawner::ExecutorThreadSpawner::new();
        let ex = crate::Executor::init_with_config($cfg);
        let res = $method(ex, $func(&mut spawner));

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
        F: FnOnce(&mut ExecutorThreadSpawner) -> Fut,
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
        F: FnOnce(&mut ExecutorThreadSpawner) -> Fut,
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

pub static DEFAULT_TEST_RUNNER: TestRunner = TestRunner::new(Config::default());

pub fn start_test_and_block_on_local<Ret, Fut, F>(func: F) -> Ret
where
    Fut: Future<Output = Ret> + 'static,
    F: FnOnce(&mut ExecutorThreadSpawner) -> Fut,
{
    DEFAULT_TEST_RUNNER.block_on_local(func)
}

pub fn start_test_and_block_on_global<Ret, Fut, F>(func: F) -> Ret
where
    Fut: Future<Output = Ret> + Send + 'static,
    F: FnOnce(&mut ExecutorThreadSpawner) -> Fut,
{
    DEFAULT_TEST_RUNNER.block_on_global(func)
}
