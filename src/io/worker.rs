use std::net::Shutdown;
use std::time::Duration;
use nix::libc;
use nix::libc::sockaddr;
use crate::io::io_request::IoRequest;
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::{RawFd, Worker as WorkerSys, OpenHow, OsMessageHeader};

#[thread_local]
pub(crate) static mut LOCAL_WORKER: Option<WorkerSys> = None;

pub(crate) unsafe fn init_local_worker() {
    unsafe {
        LOCAL_WORKER = Some(WorkerSys::new());
    }
}

#[inline(always)]
pub(crate) unsafe fn local_worker() -> &'static mut WorkerSys {
    #[cfg(debug_assertions)]
    unsafe {
        LOCAL_WORKER.as_mut().expect(crate::messages::BUG)
    }

    #[cfg(not(debug_assertions))]
    unsafe {
        LOCAL_WORKER.as_mut().unwrap_unchecked()
    }
}

pub(crate) trait IoWorker {
    fn register_time_bounded_io_task(&mut self, time_bounded_io_task: &mut TimeBoundedIoTask);
    fn deregister_time_bounded_io_task(&mut self, time_bounded_io_task: &TimeBoundedIoTask);
    /// Returns `true` if the worker has polled. The worker doesn't poll only if it has no work to do.
    #[must_use]
    fn must_poll(&mut self, duration: Duration) -> bool;
    fn socket(&mut self, domain: socket2::Domain, sock_type: socket2::Type, request_ptr: *mut IoRequest);
    fn accept(&mut self, listen_fd: RawFd, addr: *mut sockaddr, addrlen: *mut libc::socklen_t, request_ptr: *mut IoRequest);
    fn connect(&mut self, socket_fd: RawFd, addr_ptr: *const sockaddr, addr_len: libc::socklen_t, request_ptr: *mut IoRequest);
    fn poll_fd_read(&mut self, fd: RawFd, request_ptr: *mut IoRequest);
    fn poll_fd_write(&mut self, fd: RawFd, request_ptr: *mut IoRequest);
    fn recv(&mut self, fd: RawFd, buf_ptr: *mut u8, len: usize, request_ptr: *mut IoRequest);
    fn recv_from(&mut self, fd: RawFd, msg_header: *mut OsMessageHeader, request_ptr: *mut IoRequest);
    fn send(&mut self, fd: RawFd, buf_ptr: *const u8, len: usize, request_ptr: *mut IoRequest);
    fn send_to(&mut self, fd: RawFd, msg_header: *const OsMessageHeader, request_ptr: *mut IoRequest);
    fn peek(&mut self, fd: RawFd, buf_ptr: *mut u8, len: usize, request_ptr: *mut IoRequest);
    fn peek_from(&mut self, fd: RawFd, msg: *mut OsMessageHeader, request_ptr: *mut IoRequest);
    fn shutdown(&mut self, fd: RawFd, how: Shutdown, request_ptr: *mut IoRequest);
    fn open(&mut self, path: *const libc::c_char, open_how: *const OpenHow, request_ptr: *mut IoRequest);
    fn read(&mut self, fd: RawFd, buf_ptr: *mut u8, len: usize, request_ptr: *mut IoRequest);
    fn pread(&mut self, fd: RawFd, buf_ptr: *mut u8, len: usize, offset: usize, request_ptr: *mut IoRequest);
    fn write(&mut self, fd: RawFd, buf_ptr: *const u8, len: usize, request_ptr: *mut IoRequest);
    fn pwrite(&mut self, fd: RawFd, buf_ptr: *const u8, len: usize, offset: usize, request_ptr: *mut IoRequest);
    fn close(&mut self, fd: RawFd, request_ptr: *mut IoRequest);
    fn rename(&mut self, old_path:  *const libc::c_char, new_path:  *const libc::c_char, request_ptr: *mut IoRequest);
    fn create_dir(&mut self, path:  *const libc::c_char, mode: u32, request_ptr: *mut IoRequest);
    fn remove_file(&mut self, path:  *const libc::c_char, request_ptr: *mut IoRequest);
    fn remove_dir(&mut self, path:  *const libc::c_char, request_ptr: *mut IoRequest);
}