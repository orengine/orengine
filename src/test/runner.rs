//! This module provides a way to run tests with reusing
//! the same [`Executor`] via [`run_test_and_block_on_local`]
//! and [`run_test_and_block_on_shared`].
//!
//! # Example
//!
//! ```no_run
//! use orengine::test::run_test_and_block_on_local;
//!
//! async fn awesome_async_function() -> usize {
//!     42
//! }
//!
//! #[cfg(test)]
//! fn test_awesome_async_function() {
//!     run_test_and_block_on_local(async {
//!         assert_eq!(awesome_async_function().await, 42);
//!     });
//! }
//! ```
//!
//! # Shortcuts
//!
//! You can use macro [`orengine::test::test_local`](crate::test::test_local) instead
//! of [`run_test_and_block_on_local`] and [`orengine::test::test_shared`](crate::test::test_shared)
//! instead of [`run_test_and_block_on_shared`]. Read [`run_test_and_block_on_local`] and
//! [`run_test_and_block_on_shared`] for examples.
use crate::bug_message::BUG_MESSAGE;
use crate::runtime::executor::get_local_executor_ref;
use crate::runtime::Config;
use crate::{local_executor, yield_now, Executor};
use std::future::Future;

/// Prints the first test message. It contains an information about build configuration.
fn print_first_test_message() {
    #[cfg(target_os = "linux")]
    {
        println!("OS: Linux, using io-uring");
    }

    #[cfg(not(target_os = "linux"))]
    {
        #[cfg(feature = "fallback_thread_pool")]
        {
            println!("OS: Not Linux, using fallback with thread pool");
        }

        #[cfg(not(feature = "fallback_thread_pool"))]
        {
            println!("OS: Not Linux, using fallback without thread pool");
        }
    }
}

/// Initializes the local executor only if it is not initialized
/// and returns `&'static mut Executor`.
pub(crate) fn get_local_executor() -> &'static mut Executor {
    static PRINTED: std::sync::Once = std::sync::Once::new();
    PRINTED.call_once(print_first_test_message);

    if get_local_executor_ref().is_none() {
        let cfg = Config::default().disable_work_sharing();
        Executor::init_with_config(cfg);
    }

    local_executor()
}

/// Upgrades provided future to release all previous tasks.
#[allow(clippy::future_not_send, reason = "This can be non-Send")]
pub(crate) async fn upgrade_future<Fut>(future: Fut)
where
    Fut: Future<Output = ()> + 'static,
{
    while local_executor().number_of_spawned_tasks() > 0 {
        yield_now().await;
    }

    future.await;
}

/// Initializes the local executor (if it is not initialized) and blocks the current
/// thread until the `local` future is completed.
///
/// # The difference between `run_test_and_block_on_local` and [`run_test_and_block_on_shared`]
///
/// `run_test_and_block_on_local` creates a `local` task, while `run_test_and_block_on_shared`
/// creates a `shared` task.
///
/// Read more about `local` and `shared` tasks in [`Executor`].
///
/// # Example
///
/// ```no_run
/// use orengine::test::run_test_and_block_on_local;
///
/// async fn awesome_async_function() -> usize {
///     42
/// }
///
/// #[cfg(test)]
/// fn test_awesome_async_function() {
///     run_test_and_block_on_local(async {
///         assert_eq!(awesome_async_function().await, 42);
///     });
/// }
/// ```
///
/// # Shortcut
///
/// You can use [`orengine::test::test_local`](crate::test::test_local).
/// An example below is equivalent to the one above:
///
/// ```no_run
/// async fn awesome_async_function() -> usize {
///     42
/// }
///
/// #[orengine::test::test_local]
/// fn test_awesome_async_function() {
///     assert_eq!(awesome_async_function().await, 42);
/// }
/// ```
#[allow(
    clippy::missing_panics_doc,
    reason = "Panics on when a bug is occurred"
)]
pub fn run_test_and_block_on_local<Fut>(future: Fut)
where
    Fut: Future<Output = ()> + 'static,
{
    let executor = get_local_executor();
    executor
        .run_and_block_on_local(upgrade_future(future))
        .expect(BUG_MESSAGE);
}

/// Initializes the local executor (if it is not initialized) and blocks the current
/// thread until the `shared` future is completed.
///
/// # The difference between `run_test_and_block_on_shared` and [`run_test_and_block_on_local`]
///
/// `run_test_and_block_on_shared` creates a `shared` task, while `run_test_and_block_on_local`
/// creates a `local` task.
///
/// Read more about `local` and `shared` tasks in [`Executor`].
///
/// # Example
///
/// ```no_run
/// use orengine::test::run_test_and_block_on_shared;
/// # async fn get_some_result_from_shared_state() -> Result<(), ()> { Ok(()) }
///
/// async fn awesome_async_shared_function() -> usize {
///     if get_some_result_from_shared_state().await.is_err() {
///         return 0;
///     }
///
///     3
/// }
///
/// #[cfg(test)]
/// fn test_awesome_async_function() {
///     run_test_and_block_on_shared(async {
///         assert_eq!(awesome_async_shared_function().await, 3);
///     });
/// }
/// ```
///
/// # Shortcut
///
/// You can use [`orengine::test::test_shared`](crate::test::test_shared).
/// An example below is equivalent to the one above:
///
/// ```no_run
/// # async fn get_some_result_from_shared_state() -> Result<(), ()> { Ok(()) }
///
/// async fn awesome_async_shared_function() -> usize {
///     if get_some_result_from_shared_state().await.is_err() {
///         return 0;
///     }
///
///     3
/// }
///
/// #[orengine::test::test_shared]
/// fn test_awesome_async_function() {
///     assert_eq!(awesome_async_shared_function().await, 3);
/// }
/// ```
#[allow(
    clippy::missing_panics_doc,
    reason = "Panics on when a bug is occurred"
)]
pub fn run_test_and_block_on_shared<Fut>(future: Fut)
where
    Fut: Future<Output = ()> + Send + 'static,
{
    let executor = get_local_executor();
    executor
        .run_and_block_on_shared(upgrade_future(future))
        .expect(BUG_MESSAGE);
}
