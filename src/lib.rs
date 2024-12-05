#![deny(clippy::all)]
#![deny(clippy::assertions_on_result_states)]
#![deny(clippy::match_wild_err_arm)]
#![deny(clippy::allow_attributes_without_reason)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]
#![allow(async_fn_in_trait, reason = "It improves readability.")]
#![allow(
    clippy::missing_const_for_fn,
    reason = "Since we cannot make a constant function non-constant after its release,
    we need to look for a reason to make it constant, and not vice versa."
)]
#![allow(clippy::inline_always, reason = "We write highly optimized code.")]
#![allow(
    clippy::must_use_candidate,
    reason = "It is better to developer think about it."
)]
#![allow(
    clippy::module_name_repetitions,
    reason = "This is acceptable most of the time."
)]
#![allow(
    clippy::missing_errors_doc,
    reason = "Unless the error is something special,
    the developer should document it."
)]
#![allow(clippy::redundant_pub_crate, reason = "It improves readability.")]
#![allow(clippy::struct_field_names, reason = "It improves readability.")]
#![allow(
    clippy::module_inception,
    reason = "It is fine if a file in has the same mane as a module."
)]
#![allow(clippy::if_not_else, reason = "It improves readability.")]
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
