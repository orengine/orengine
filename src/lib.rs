#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(waker_getters)]
#![feature(slice_ptr_get)]
#![feature(ptr_as_ref_unchecked)]
#![feature(io_error_uncategorized)]
#![allow(async_fn_in_trait)]
#![feature(async_closure)]
#![feature(negative_impls)]
#![feature(trait_alias)]
#![feature(thread_local)]
#![feature(const_collections_with_hasher)]

// TODO rename lifetimes

pub use socket2;

pub use end::end_local_thread;
pub use run::*;
pub use runtime::Executor;
pub use yield_now::yield_now;

pub mod buf;
pub mod cfg;
pub mod end;
pub mod fs;
pub mod io;
pub mod local;
mod local_pool;
pub mod net;
pub mod run;
pub mod runtime;
pub mod sleep;
pub mod sync;
pub mod utils;
pub mod yield_now;
mod messages;
