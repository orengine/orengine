#![allow(internal_features)]
#![allow(async_fn_in_trait)]
#![feature(core_intrinsics)]
#![feature(waker_getters)]
#![feature(async_closure)]
#![feature(negative_impls)]
#![feature(thread_local)]
#![feature(io_error_uncategorized)]

pub mod buf;
pub mod cfg;
pub mod end;
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
pub use end::{end_local_thread, end};
pub use run::*;
pub use runtime::Executor;
pub use yield_now::yield_now;
pub use sleep::sleep;