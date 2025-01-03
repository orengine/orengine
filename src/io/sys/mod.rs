//! Helper for work with the system.
#[cfg(unix)]
pub mod unix;
#[cfg(windows)]
pub(crate) mod windows;

pub(crate) use io_uring::types::OpenHow;
#[cfg(unix)]
pub(crate) use unix::fd::*;
#[cfg(target_os = "linux")]
pub(crate) use unix::os_message_header::*;
#[cfg(target_os = "linux")]
pub(crate) use unix::os_path as OsPath;
#[cfg(unix)]
pub use unix::IOUringConfig;
#[cfg(target_os = "linux")]
pub(crate) use unix::IOUringWorker as WorkerSys;
#[cfg(windows)]
pub(crate) use windows::fd::*;
