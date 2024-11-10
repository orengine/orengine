//! This module contains utilities for testing such as
//!
//! - [`test_shared`] and [`test_local`] macros reexports from `orengine-macros` to create tests
//! with concise code;
//!
//! - [`executor_pool`] that contains utilities for parallel testing via
//! [`sched_future_to_another_thread`] or [`sched_future`](ExecutorPool::sched_future);
//!
//! - [`runner`] that provides a way to run tests with reusing
//! the same [`Executor`](crate::Executor) via [`run_test_and_block_on_local`]
//! and [`run_test_and_block_on_shared`].
//!
//! # Examples
//!
//! You can find example in the [`examples`](https://github.com/orengine/orengine/tree/main/examples)
//! folder.
//!
//! # How to write parallel tests?
//!
//! If you want to write parallel tests, you can use [`sched_future_to_another_thread`]
//! or [`sched_future`](ExecutorPool::sched_future).

pub mod executor_pool;
pub mod runner;

pub use executor_pool::*;
pub use orengine_macros::{test_shared, test_local};
pub use runner::*;
