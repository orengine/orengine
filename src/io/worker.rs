use std::net::Shutdown;
use std::ptr::addr_of_mut;
use std::time::Duration;
use nix::libc;
use nix::libc::sockaddr;
use crate::io::config::IoWorkerConfig;
use crate::io::io_request_data::IoRequestData;
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::{RawFd, WorkerSys, OpenHow, OsMessageHeader};
use crate::BUG_MESSAGE;

/// Thread-local worker for async io operations.
#[thread_local]
pub(crate) static mut LOCAL_WORKER: Option<WorkerSys> = None;

/// Returns the thread-local worker wrapped in an [`Option`].
pub(crate) fn get_local_worker_ref() -> &'static mut Option<WorkerSys> {
    unsafe { &mut *(&raw mut LOCAL_WORKER) }
}

/// Initializes the thread-local worker.
pub(crate) unsafe fn init_local_worker(config: IoWorkerConfig) {
    if get_local_worker_ref().is_some() {
        panic!("{}", BUG_MESSAGE);
    }
    
    *get_local_worker_ref() = Some(WorkerSys::new(config));
}

/// Returns the thread-local worker wrapped in an [`Option`].
pub(crate) fn local_worker_option() -> &'static mut Option<WorkerSys> {
    unsafe { &mut *addr_of_mut!(LOCAL_WORKER) }
}

/// Returns the thread-local worker.
///
/// # Panics
///
/// If the thread-local worker has not been initialized.
///
/// # Undefined Behavior
///
/// If the thread-local worker has not been initialized in `release` mode.
#[inline(always)]
pub(crate) unsafe fn local_worker() -> &'static mut WorkerSys {
    #[cfg(debug_assertions)]
    {
        if crate::local_executor().config().io_worker_config().is_none() {
            panic!("An attempt to call io-operation has failed, \
             because an Executor has no io-worker. Look at the config of the Executor.");
        }

        get_local_worker_ref().as_mut().expect(BUG_MESSAGE)
    }

    #[cfg(not(debug_assertions))]
    unsafe {
        get_local_worker_ref().as_mut().unwrap_unchecked()
    }
}

/// A worker for async io operations.
pub(crate) trait IoWorker {
    /// Creates a new worker.
    fn new(config: IoWorkerConfig) -> Self;
    /// Registers a new time-bounded io task. It will be cancelled if the deadline is reached.
    fn register_time_bounded_io_task(&mut self, time_bounded_io_task: &mut TimeBoundedIoTask);
    /// Deregisters a time-bounded io task. It is used to say [`IoWorker`] to not cancel the task.
    fn deregister_time_bounded_io_task(&mut self, time_bounded_io_task: &TimeBoundedIoTask);
    /// Submits an accumulated tasks to the kernel and polls it for completion if needed.
    ///
    /// Returns `true` if the worker has polled.
    /// The worker doesn't poll only if it has no work to do.
    #[must_use]
    fn must_poll(&mut self, duration: Duration) -> bool;
    /// Registers a new `socket` io operation.
    fn socket(&mut self, domain: socket2::Domain, sock_type: socket2::Type, request_ptr: *mut IoRequestData);
    /// Registers a new `accept` io operation.
    fn accept(&mut self, listen_fd: RawFd, addr: *mut sockaddr, addrlen: *mut libc::socklen_t, request_ptr: *mut IoRequestData);
    /// Registers a new `connect` io operation.
    fn connect(&mut self, socket_fd: RawFd, addr_ptr: *const sockaddr, addr_len: libc::socklen_t, request_ptr: *mut IoRequestData);
    /// Registers a new `poll` for readable io operation.
    fn poll_fd_read(&mut self, fd: RawFd, request_ptr: *mut IoRequestData);
    /// Registers a new `poll` for writable io operation.
    fn poll_fd_write(&mut self, fd: RawFd, request_ptr: *mut IoRequestData);
    /// Registers a new `recv` io operation.
    fn recv(&mut self, fd: RawFd, buf_ptr: *mut u8, len: usize, request_ptr: *mut IoRequestData);
    /// Registers a new `recv_from` io operation.
    fn recv_from(&mut self, fd: RawFd, msg_header: *mut OsMessageHeader, request_ptr: *mut IoRequestData);
    /// Registers a new `send` io operation.
    fn send(&mut self, fd: RawFd, buf_ptr: *const u8, len: usize, request_ptr: *mut IoRequestData);
    /// Registers a new `send_to` io operation.
    fn send_to(&mut self, fd: RawFd, msg_header: *const OsMessageHeader, request_ptr: *mut IoRequestData);
    /// Registers a new `peek` io operation.
    fn peek(&mut self, fd: RawFd, buf_ptr: *mut u8, len: usize, request_ptr: *mut IoRequestData);
    /// Registers a new `peek_from` io operation.
    fn peek_from(&mut self, fd: RawFd, msg: *mut OsMessageHeader, request_ptr: *mut IoRequestData);
    /// Registers a new `shutdown` io operation.
    fn shutdown(&mut self, fd: RawFd, how: Shutdown, request_ptr: *mut IoRequestData);
    /// Registers a new `open` io operation.
    fn open(&mut self, path: *const libc::c_char, open_how: *const OpenHow, request_ptr: *mut IoRequestData);
    /// Registers a new `fallocate` io operation if the kernel supports it.
    fn fallocate(&mut self, fd: RawFd, offset:u64, len: u64, flags: i32, request_ptr: *mut IoRequestData);
    /// Registers a new `sync_all` io operation.
    fn sync_all(&mut self, fd: RawFd, request_ptr: *mut IoRequestData);
    /// Registers a new `sync_data` io operation.
    fn sync_data(&mut self, fd: RawFd, request_ptr: *mut IoRequestData);
    /// Registers a new `read` io operation.
    fn read(&mut self, fd: RawFd, buf_ptr: *mut u8, len: usize, request_ptr: *mut IoRequestData);
    /// Registers a new `pread` io operation if the kernel supports it else uses cursors.
    fn pread(&mut self, fd: RawFd, buf_ptr: *mut u8, len: usize, offset: usize, request_ptr: *mut IoRequestData);
    /// Registers a new `write` io operation.
    fn write(&mut self, fd: RawFd, buf_ptr: *const u8, len: usize, request_ptr: *mut IoRequestData);
    /// Registers a new `pwrite` io operation if the kernel supports it else uses cursors.
    fn pwrite(&mut self, fd: RawFd, buf_ptr: *const u8, len: usize, offset: usize, request_ptr: *mut IoRequestData);
    /// Registers a new `close` io operation.
    fn close(&mut self, fd: RawFd, request_ptr: *mut IoRequestData);
    /// Registers a new `rename` io operation.
    fn rename(&mut self, old_path:  *const libc::c_char, new_path:  *const libc::c_char, request_ptr: *mut IoRequestData);
    /// Registers a new `mkdir` io operation.
    fn create_dir(&mut self, path:  *const libc::c_char, mode: u32, request_ptr: *mut IoRequestData);
    /// Registers a new `unlink` io operation.
    fn remove_file(&mut self, path:  *const libc::c_char, request_ptr: *mut IoRequestData);
    /// Registers a new `rmdir` io operation.
    fn remove_dir(&mut self, path:  *const libc::c_char, request_ptr: *mut IoRequestData);
}