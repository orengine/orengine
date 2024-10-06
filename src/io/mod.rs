//! This module contains async io operations, utils for working with them and structs
//! for working with them like [`IoUringWorker`](sys::unix::IoUringWorker).
//!
//! It also contains [`IoWorker`](worker::IoWorker) trait.
pub(crate) mod close;
pub mod config;
pub mod fs;
pub(crate) mod io_request_data;
pub mod net;
pub(crate) mod sys;
pub(crate) mod time_bounded_io_task;
pub(crate) mod worker;

pub use close::AsyncClose;
pub use config::IoWorkerConfig;
pub use fs::*;
pub use net::*;
