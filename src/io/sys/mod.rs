//! Helper for work with the system.
#[cfg(unix)]
pub(crate) mod unix;
#[cfg(windows)]
pub(crate) mod windows;

pub(crate) use io_uring::types::OpenHow;
#[cfg(unix)]
pub(crate) use unix::fd::*;
pub(crate) use unix::os_message_header::*;
pub(crate) use unix::os_path as OsPath;
pub(crate) use unix::IoUringWorker as WorkerSys;
#[cfg(windows)]
pub(crate) use windows::fd::*;
