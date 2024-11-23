#![allow(internal_features)]
#![allow(async_fn_in_trait)]
#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::inline_always)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::redundant_pub_crate)]
#![allow(clippy::struct_field_names)]
#![deny(clippy::allow_attributes_without_reason)]
#![deny(clippy::assertions_on_result_states)]
#![deny(clippy::match_wild_err_arm)]
#![allow(clippy::module_inception)]
#![allow(clippy::if_not_else)]
pub mod buf;
pub(crate) mod bug_message;
pub mod fs;
pub mod io;
pub mod local;
pub mod local_pool;
pub mod net;
pub mod run;
pub mod runtime;
pub mod sleep;
pub mod sync;
pub mod sync_task_queue;
pub mod test;
pub mod utils;
pub mod yield_now;

pub(crate) use bug_message::BUG_MESSAGE;
pub use local::Local;
pub use run::*;
pub use runtime::{asyncify, local_executor, stop_all_executors, stop_executor, Executor};
pub use sleep::sleep;
pub use socket2;
pub use yield_now::yield_now;
