#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(waker_getters)]
#![feature(slice_ptr_get)]
#![feature(ptr_as_ref_unchecked)]
#![feature(io_error_uncategorized)]
#![allow(async_fn_in_trait)]
#![feature(async_closure)]
#![feature(negative_impls)]

pub mod utils;
pub mod runtime;
pub mod yield_now;
pub mod sleep;
pub mod end;
pub mod cfg;
pub mod buf;
pub mod local;
pub mod io;
pub mod fs;
pub mod net;
pub mod run;
pub mod sync;

pub use runtime::Executor;
pub use yield_now::yield_now;
pub use end::end_local_thread;
pub use run::*;