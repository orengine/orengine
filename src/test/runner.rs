// TODO docs

use crate::bug_message::BUG_MESSAGE;
use crate::runtime::executor::get_local_executor_ref;
use crate::runtime::Config;
use crate::{local_executor, Executor};
use std::future::Future;

pub struct TestRunner {}

impl TestRunner {
    pub const fn new() -> Self {
        Self {}
    }

    pub fn get_local_executor(&self) -> &'static mut Executor {
        if get_local_executor_ref().is_none() {
            let cfg = Config::default().disable_work_sharing();
            Executor::init_with_config(cfg);
        }

        local_executor()
    }

    pub fn block_on_local<Fut>(&self, future: Fut)
    where
        Fut: Future<Output = ()> + 'static,
    {
        let executor = self.get_local_executor();
        executor.run_and_block_on_local(future).expect(BUG_MESSAGE);
    }

    pub fn block_on_global<Fut>(&self, future: Fut)
    where
        Fut: Future<Output = ()> + Send + 'static,
    {
        let executor = self.get_local_executor();
        executor.run_and_block_on_global(future).expect(BUG_MESSAGE);
    }
}

thread_local! {
    static LOCAL_TEST_RUNNER: TestRunner = TestRunner::new();
}

pub fn run_test_and_block_on_local<Fut>(future: Fut)
where
    Fut: Future<Output = ()> + 'static,
{
    LOCAL_TEST_RUNNER.with(|runner| runner.block_on_local(future));
}

pub fn run_test_and_block_on_global<Fut>(future: Fut)
where
    Fut: Future<Output = ()> + Send + 'static,
{
    LOCAL_TEST_RUNNER.with(|runner| runner.block_on_global(future));
}
