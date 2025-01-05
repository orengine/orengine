//! Helper for work with the system.
#[cfg(not(target_os = "linux"))]
pub(crate) mod fallback;

#[cfg(target_os = "linux")]
pub(crate) mod linux;

pub mod sockets_and_files;
pub mod worker_configs;

pub use sockets_and_files::*;
pub use worker_configs::*;

#[cfg(not(windows))]
pub use libc::sockaddr_storage;
#[cfg(not(windows))]
pub use libc::socklen_t;

#[cfg(windows)]
pub use windows_sys::Win32::Networking::WinSock as sockaddr_storage;
#[cfg(windows)]
pub use windows_sys::Win32::Networking::WinSock::socklen_t;

#[cfg(target_os = "linux")]
pub(crate) use libc::sockaddr as os_sockaddr;
#[cfg(target_os = "linux")]
pub(crate) use linux::open_options::OsOpenOptions;
#[cfg(target_os = "linux")]
pub(crate) use linux::os_message_header::*;
#[cfg(target_os = "linux")]
pub(crate) use linux::os_path::{get_os_path, get_os_path_ptr, OsPath, OsPathPtr};
#[cfg(target_os = "linux")]
pub(crate) use linux::IOUringWorker as WorkerSys;

#[cfg(not(target_os = "linux"))]
pub(crate) use fallback::open_options::OsOpenOptions;
#[cfg(not(target_os = "linux"))]
pub(crate) use fallback::os_message_header::*;
#[cfg(not(target_os = "linux"))]
pub(crate) use fallback::os_path::{get_os_path, get_os_path_ptr, OsPath, OsPathPtr};
#[cfg(not(target_os = "linux"))]
pub(crate) use fallback::with_thread_pool::FallbackWorker as WorkerSys;
#[cfg(not(target_os = "linux"))]
pub(crate) use sockaddr_storage as os_sockaddr;
