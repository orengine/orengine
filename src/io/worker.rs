use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::net::Shutdown;
use nix::libc;
use nix::libc::sockaddr;
use crate::io::io_request::IoRequest;
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::{Fd, Worker as WorkerSys, OpenHow, OsMessageHeader};

thread_local! {
    pub(crate) static LOCAL_WORKER: UnsafeCell<MaybeUninit<WorkerSys>> = UnsafeCell::new(MaybeUninit::uninit());
}

pub(crate) unsafe fn init_local_worker() {
    unsafe {
        LOCAL_WORKER.with(|local_worker| {
            local_worker.get().write(MaybeUninit::new(WorkerSys::new()));
        })
    }
}

#[inline(always)]
pub(crate) unsafe fn local_worker() -> &'static mut WorkerSys {
    unsafe {
        LOCAL_WORKER.with(|local_worker| {
            (&mut *local_worker.get()).assume_init_mut()
        })
    }
}

pub(crate) trait IoWorker {
    fn register_time_bounded_io_task(&mut self, time_bounded_io_task: &mut TimeBoundedIoTask);
    fn deregister_time_bounded_io_task(&mut self, time_bounded_io_task: &TimeBoundedIoTask);
    fn must_poll(&mut self);
    fn accept(&mut self, listen_fd: Fd, addr: *mut sockaddr, addrlen: *mut libc::socklen_t, request_ptr: *const IoRequest);
    fn connect(&mut self, socket_fd: Fd, addr_ptr: *const sockaddr, addr_len: libc::socklen_t, request_ptr: *const IoRequest);
    fn poll_fd_read(&mut self, fd: Fd, request_ptr: *const IoRequest);
    fn poll_fd_write(&mut self, fd: Fd, request_ptr: *const IoRequest);
    fn recv(&mut self, fd: Fd, buf_ptr: *mut u8, len: usize, request_ptr: *const IoRequest);
    fn recv_from(&mut self, fd: Fd, msg_header: *mut OsMessageHeader, request_ptr: *const IoRequest);
    fn send(&mut self, fd: Fd, buf_ptr: *const u8, len: usize, request_ptr: *const IoRequest);
    fn send_to(&mut self, fd: Fd, msg_header: *const OsMessageHeader, request_ptr: *const IoRequest);
    fn peek(&mut self, fd: Fd, buf_ptr: *mut u8, len: usize, request_ptr: *const IoRequest);
    fn peek_from(&mut self, fd: Fd, msg: *mut OsMessageHeader, request_ptr: *const IoRequest);
    fn shutdown(&mut self, fd: Fd, how: Shutdown, request_ptr: *const IoRequest);
    fn open(&mut self, path: *const libc::c_char, open_how: *const OpenHow, request_ptr: *const IoRequest);
    fn read(&mut self, fd: Fd, buf_ptr: *mut u8, len: usize, request_ptr: *const IoRequest);
    fn pread(&mut self, fd: Fd, buf_ptr: *mut u8, len: usize, offset: usize, request_ptr: *const IoRequest);
    fn write(&mut self, fd: Fd, buf_ptr: *const u8, len: usize, request_ptr: *const IoRequest);
    fn pwrite(&mut self, fd: Fd, buf_ptr: *const u8, len: usize, offset: usize, request_ptr: *const IoRequest);
    fn close(&mut self, fd: Fd, request_ptr: *const IoRequest);
    fn rename(&mut self, old_path:  *const libc::c_char, new_path:  *const libc::c_char, request_ptr: *const IoRequest);
    fn create_dir(&mut self, path:  *const libc::c_char, mode: u32, request_ptr: *const IoRequest);
    fn remove_file(&mut self, path:  *const libc::c_char, request_ptr: *const IoRequest);
    fn remove_dir(&mut self, path:  *const libc::c_char, request_ptr: *const IoRequest);
}