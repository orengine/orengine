pub mod executor;
pub mod waker;
pub mod task;
pub mod call;
pub mod executors_on_cores_table;
pub mod config;
mod global_state;

pub use executor::{
    local_executor,
    local_executor_unchecked,
    Executor
};

pub use config::Config;
pub(crate) use task::*;
pub use global_state::{stop_executor, stop_all_executors};
pub use executors_on_cores_table::get_core_id_for_executor;