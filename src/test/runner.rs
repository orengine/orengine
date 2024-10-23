use crate::bug_message::BUG_MESSAGE;
use crate::runtime::executor::get_local_executor_ref;
use crate::runtime::Config;
use crate::{local_executor, Executor};
use std::future::Future;

/// `TestRunner` provides a way to run tests with reusing the same [`Executor`].
/// It creates only one [`Executor`] per thread and allows working with it via
/// [`block_on_local`](TestRunner::block_on_local)
/// and [`block_on_global`](TestRunner::block_on_global).
///
/// Please use it in tests, because it is efficient.
///
/// # Thread safety
///
/// `TestRunner` is thread-safe because it is stored in `thread_local`.
pub struct TestRunner {}

impl TestRunner {
    /// Initializes the local executor only if it is not initialized
    /// and returns `&'static mut Executor`.
    pub(crate) fn get_local_executor(&self) -> &'static mut Executor {
        if get_local_executor_ref().is_none() {
            let cfg = Config::default().disable_work_sharing();
            Executor::init_with_config(cfg);
        }

        local_executor()
    }

    /// Initializes the local executor (if it is not initialized) and blocks the current
    /// thread until the `local` future is completed.
    ///
    /// # The difference between `block_on_local` and [`block_on_global`](TestRunner::block_on_global)
    ///
    /// `block_on_local` creates a `local` task, while `block_on_global` creates a `global` task.
    ///
    /// Read more about `local` and `global` tasks in [`Executor`].
    pub(crate) fn block_on_local<Fut>(&self, future: Fut)
    where
        Fut: Future<Output = ()> + 'static,
    {
        let executor = self.get_local_executor();
        executor.run_and_block_on_local(future).expect(BUG_MESSAGE);
    }

    /// Initializes the local executor (if it is not initialized) and blocks the current
    /// thread until the `global` future is completed.
    ///
    /// # The difference between `block_on_global` and [`block_on_local`](TestRunner::block_on_local)
    ///
    /// `block_on_global` creates a `global` task, while `block_on_local` creates a `local` task.
    ///
    /// Read more about `local` and `global` tasks in [`Executor`].
    pub(crate) fn block_on_global<Fut>(&self, future: Fut)
    where
        Fut: Future<Output = ()> + Send + 'static,
    {
        let executor = self.get_local_executor();
        executor.run_and_block_on_global(future).expect(BUG_MESSAGE);
    }
}

thread_local! {
    /// Thread-local [`TestRunner`].
    static LOCAL_TEST_RUNNER: TestRunner = TestRunner {};
}

/// Initializes the local executor (if it is not initialized) and blocks the current
/// thread until the `local` future is completed.
///
/// # The difference between `run_test_and_block_on_local` and [`run_test_and_block_on_global`](run_test_and_block_on_global)
///
/// `run_test_and_block_on_local` creates a `local` task, while `run_test_and_block_on_global`
/// creates a `global` task.
///
/// Read more about `local` and `global` tasks in [`Executor`].
pub fn run_test_and_block_on_local<Fut>(future: Fut)
where
    Fut: Future<Output = ()> + 'static,
{
    LOCAL_TEST_RUNNER.with(|runner| runner.block_on_local(future));
}

/// Initializes the local executor (if it is not initialized) and blocks the current
/// thread until the `global` future is completed.
///
/// # The difference between `run_test_and_block_on_global` and [`run_test_and_block_on_local`](run_test_and_block_on_local)
///
/// `run_test_and_block_on_global` creates a `global` task, while `run_test_and_block_on_local`
/// creates a `local` task.
///
/// Read more about `local` and `global` tasks in [`Executor`].
pub fn run_test_and_block_on_global<Fut>(future: Fut)
where
    Fut: Future<Output = ()> + Send + 'static,
{
    LOCAL_TEST_RUNNER.with(|runner| runner.block_on_global(future));
}
