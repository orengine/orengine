#![allow(internal_features)]
#![allow(async_fn_in_trait)]
#![feature(core_intrinsics)]
#![feature(async_closure)]
#![feature(negative_impls)]
#![feature(thread_local)]
#![feature(io_error_uncategorized)]

pub mod buf;
pub mod fs;
pub mod io;
pub mod local;
pub mod local_pool;
pub mod net;
pub mod run;
pub mod runtime;
pub mod sleep;
pub mod sync;
pub mod utils;
pub mod yield_now;
mod messages;
pub mod atomic_task_queue;

pub use socket2;
pub use run::*;
pub use runtime::{local_executor, Executor, stop_all_executors, stop_executor, asyncify};
pub use yield_now::{global_yield_now, local_yield_now};
pub use sleep::sleep;