//! Helper for work with the system.
#[cfg(not(target_os = "linux"))]
pub(crate) mod fallback;
pub mod sockets_and_files;
#[cfg(unix)]
pub(crate) mod unix;
pub mod worker_configs;

pub use sockets_and_files::*;
pub use worker_configs::*;

#[cfg(unix)]
pub(crate) use io_uring::types::OpenHow;
#[cfg(target_os = "linux")]
pub(crate) use unix::os_message_header::*;
#[cfg(target_os = "linux")]
pub(crate) use unix::os_path;
#[cfg(target_os = "linux")]
pub(crate) use unix::IOUringWorker as WorkerSys;

#[cfg(not(target_os = "linux"))]
pub(crate) use fallback::os_path;
#[cfg(not(target_os = "linux"))]
pub(crate) use fallback::FallbackWorker as WorkerSys;
