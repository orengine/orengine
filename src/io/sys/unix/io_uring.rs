use crate::io::config::IoWorkerConfig;
use crate::io::io_request_data::IoRequestData;
use crate::io::sys::{IntoRawFd, MessageRecvHeader, OsMessageHeader, RawFd};
use crate::io::time_bounded_io_task::TimeBoundedIoTask;
use crate::io::worker::IoWorker;
use crate::runtime::local_executor;
use crate::BUG_MESSAGE;
use io_uring::squeue::Entry;
use io_uring::types::{OpenHow, SubmitArgs, Timespec};
use io_uring::{cqueue, opcode, types, IoUring, Probe};
use nix::libc;
use nix::libc::sockaddr;
use std::cell::UnsafeCell;
use std::collections::{BTreeSet, VecDeque};
use std::ffi::c_int;
use std::io::{Error, ErrorKind};
use std::net::Shutdown;
use std::thread;
use std::time::{Duration, Instant};

/// [`IOUringWorker`] implements [`IoWorker`] using `io_uring`.
pub(crate) struct IOUringWorker {
    /// # Why we need some cell?
    ///
    /// We can't rewrite engine ([`Selector`] trait) to use separately `ring` field and other fields in different methods.
    /// For example, we can't use `&mut self` in [`Scheduler::handle_coroutine_state`] function, because we are borrowing the `ring` before [`Scheduler::handle_coroutine_state`].
    /// So we need some cell not to destroy the abstraction.
    ///
    /// # Why we use [`UnsafeCell`]?
    ///
    /// Because we can guarantee that:
    /// * only one thread can borrow the [`IOUringWorker`] at the same time
    /// * only in the [`poll`] method we borrow the `ring` field for [`CompletionQueue`] and [`SubmissionQueue`],
    ///   but only after the [`SubmissionQueue`] is submitted we start using the [`CompletionQueue`] that can call the [`IOUringWorker::push_sqe`]
    ///   but it is safe, because the [`SubmissionQueue`] has already been read and submitted.
    ring: UnsafeCell<IoUring<Entry, cqueue::Entry>>,
    backlog: VecDeque<Entry>,
    probe: Probe,
    time_bounded_io_task_queue: BTreeSet<TimeBoundedIoTask>,
    number_of_active_tasks: usize,
}

/// User data for [`AsyncClose`](opcode::AsyncCancel) operations.
const ASYNC_CLOSE_DATA: u64 = u64::MAX;

impl IOUringWorker {
    /// Get whether a specific opcode is supported.
    #[inline(always)]
    fn is_supported(&self, opcode: u8) -> bool {
        self.probe.is_supported(opcode)
    }

    /// Add a new sqe to the submission queue.
    #[inline(always)]
    fn add_sqe(&mut self, sqe: Entry) {
        self.number_of_active_tasks += 1;
        let ring = unsafe { &mut *self.ring.get() };
        unsafe {
            if ring.submission().push(&sqe).is_err() {
                self.backlog.push_back(sqe);
            }
        }
    }

    /// Add a new sqe to the submission queue with setting `user_data`.
    #[inline(always)]
    fn register_entry_with_u64_data(&mut self, sqe: Entry, data: u64) {
        let sqe = sqe.user_data(data);
        self.add_sqe(sqe);
    }

    /// Add a new sqe to the submission queue with setting `user_data`.
    #[inline(always)]
    fn register_entry(&mut self, sqe: Entry, data: *mut IoRequestData) {
        self.register_entry_with_u64_data(sqe, data as _);
    }

    /// Cancels a request with the given `user_data`.
    #[inline(always)]
    fn cancel_entry(&mut self, data: u64) {
        self.add_sqe(
            opcode::AsyncCancel::new(data)
                .build()
                .user_data(ASYNC_CLOSE_DATA),
        );
    }

    /// Cancels requests that have expired.
    #[inline(always)]
    fn check_deadlines(&mut self) {
        if self.time_bounded_io_task_queue.is_empty() {
            return;
        }

        let now = Instant::now();

        while let Some(time_bounded_io_task) = self.time_bounded_io_task_queue.pop_first() {
            if time_bounded_io_task.deadline() <= now {
                self.cancel_entry(time_bounded_io_task.user_data());
            } else {
                self.time_bounded_io_task_queue.insert(time_bounded_io_task);
                break;
            }
        }
    }

    /// Submits all accumulated requests and waits for completions or a timeout.
    ///
    /// Returns Ok(false) if the submission queue is empty and no sqes were submitted at all.
    #[inline(always)]
    fn submit_and_poll(&mut self, timeout_option: Option<Duration>) -> Result<bool, Error> {
        if self.number_of_active_tasks == 0 {
            if let Some(timeout) = timeout_option {
                thread::sleep(timeout);
            }
            return Ok(false);
        }

        let ring = unsafe { &mut *self.ring.get() };
        let mut sq = unsafe { ring.submission_shared() };
        let submitter = ring.submitter();

        loop {
            if sq.is_full() {
                match submitter.submit() {
                    Ok(_) => (),
                    Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => break,
                    Err(err) => return Err(err),
                }
            }
            sq.sync();

            match self.backlog.pop_front() {
                Some(sqe) => unsafe {
                    let _ = sq.push(&sqe);
                },
                None => break,
            }
        }

        let res = if let Some(timeout) = timeout_option {
            let timespec = Timespec::new().nsec(timeout.as_nanos() as u32);
            let args = SubmitArgs::new().timespec(&timespec);
            submitter.submit_with_args(1, &args)
        } else {
            submitter.submit()
        };

        match res {
            Ok(_) => (),
            Err(ref err) if err.raw_os_error() == Some(libc::ETIME) => (),
            Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => (),
            Err(err) => return Err(err),
        }

        Ok(true)
    }
}

// TODO opcode::ProvideBuffers. Read tokio-uring::io::pool for more information

impl IoWorker for IOUringWorker {
    fn new(config: IoWorkerConfig) -> Self {
        let mut s = Self {
            ring: UnsafeCell::new(IoUring::new(config.io_uring.number_of_entries).unwrap()),
            backlog: VecDeque::new(),
            probe: Probe::new(),
            time_bounded_io_task_queue: BTreeSet::new(),
            number_of_active_tasks: 0,
        };

        let submitter = s.ring.get_mut().submitter();
        submitter.register_probe(&mut s.probe).expect(BUG_MESSAGE);

        s
    }

    #[inline(always)]
    fn register_time_bounded_io_task(
        &mut self,
        io_request_data: &IoRequestData,
        deadline: &mut Instant,
    ) {
        let mut time_bounded_io_task = TimeBoundedIoTask::new(io_request_data, *deadline);
        while !self.time_bounded_io_task_queue.insert(time_bounded_io_task) {
            *deadline += Duration::from_nanos(1);
            time_bounded_io_task = TimeBoundedIoTask::new(io_request_data, *deadline);
        }
    }

    #[inline(always)]
    fn deregister_time_bounded_io_task(&mut self, deadline: &Instant) {
        self.time_bounded_io_task_queue.remove(deadline);
    }

    #[inline(always)]
    #[must_use]
    fn must_poll(&mut self, timeout_option: Option<Duration>) -> bool {
        self.check_deadlines();
        if !self
            .submit_and_poll(timeout_option)
            .expect("IOUringWorker::submit() failed")
        {
            return false;
        }

        let ring = unsafe { &mut *self.ring.get() };
        let mut cq = ring.completion();
        cq.sync();
        let executor = local_executor();

        for cqe in &mut cq {
            self.number_of_active_tasks -= 1;
            if cqe.user_data() == ASYNC_CLOSE_DATA {
                continue;
            }

            let ret = cqe.result();
            let io_request_ptr = cqe.user_data() as *mut IoRequestData;
            let io_request = unsafe { &mut *io_request_ptr };

            if ret < 0 {
                if ret == -libc::ECANCELED {
                    io_request.set_ret(Err(Error::from(ErrorKind::TimedOut)));
                } else {
                    io_request.set_ret(Err(Error::from_raw_os_error(-ret)));
                }
            } else {
                #[allow(clippy::cast_sign_loss, reason = "the sing was checked above")]
                io_request.set_ret(Ok(ret as _));
            }

            let task = unsafe { io_request.task() };
            if task.is_local() {
                executor.exec_task(task);
            } else {
                executor.spawn_shared_task(task);
            }
        }

        true
    }

    #[inline(always)]
    fn socket(
        &mut self,
        domain: socket2::Domain,
        sock_type: socket2::Type,
        protocol: socket2::Protocol,
        request_ptr: *mut IoRequestData,
    ) {
        if self.is_supported(opcode::Socket::CODE) {
            self.register_entry(
                opcode::Socket::new(
                    c_int::from(domain),
                    c_int::from(sock_type),
                    c_int::from(protocol),
                )
                .build(),
                request_ptr,
            );

            return;
        }

        let request = unsafe { &mut *request_ptr };
        let socket_ = socket2::Socket::new(domain, sock_type, Some(protocol));
        #[allow(
            clippy::cast_sign_loss,
            reason = "the sing was checked by map() method"
        )]
        request.set_ret(socket_.map(|s| s.into_raw_fd() as usize));

        local_executor().spawn_local_task(unsafe { request.task() });
    }

    #[inline(always)]
    fn accept(
        &mut self,
        listen_fd: RawFd,
        addr: *mut sockaddr,
        addrlen: *mut libc::socklen_t,
        request_ptr: *mut IoRequestData,
    ) {
        self.register_entry(
            opcode::Accept::new(types::Fd(listen_fd), addr, addrlen).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn connect(
        &mut self,
        socket_fd: RawFd,
        addr_ptr: *const sockaddr,
        addr_len: libc::socklen_t,
        request_ptr: *mut IoRequestData,
    ) {
        self.register_entry(
            opcode::Connect::new(types::Fd(socket_fd), addr_ptr, addr_len).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn poll_fd_read(&mut self, fd: RawFd, request_ptr: *mut IoRequestData) {
        self.register_entry(
            opcode::PollAdd::new(types::Fd(fd), libc::POLLIN as _).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn poll_fd_write(&mut self, fd: RawFd, request_ptr: *mut IoRequestData) {
        self.register_entry(
            opcode::PollAdd::new(types::Fd(fd), libc::POLLOUT as _).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn recv(&mut self, fd: RawFd, buf_ptr: *mut u8, len: usize, request_ptr: *mut IoRequestData) {
        self.register_entry(
            #[allow(clippy::cast_possible_truncation, reason = "we have to cast it")]
            opcode::Recv::new(types::Fd(fd), buf_ptr, len as _).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn recv_from(
        &mut self,
        fd: RawFd,
        msg_header: &mut MessageRecvHeader,
        request_ptr: *mut IoRequestData,
    ) {
        self.register_entry(
            opcode::RecvMsg::new(types::Fd(fd), &mut msg_header.header).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn send(&mut self, fd: RawFd, buf_ptr: *const u8, len: usize, request_ptr: *mut IoRequestData) {
        // TODO https://github.com/tokio-rs/io-uring/issues/308
        // if self.is_supported(opcode::SendZc::CODE) {
        //     self.register_entry(
        //         opcode::SendZc::new(types::Fd(fd), buf_ptr, len as _).build(),
        //         request_ptr,
        //     );
        //     return;
        // }

        self.register_entry(
            #[allow(clippy::cast_possible_truncation, reason = "we have to cast it")]
            opcode::Send::new(types::Fd(fd), buf_ptr, len as _).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn send_to(
        &mut self,
        fd: RawFd,
        msg_header: *const OsMessageHeader,
        request_ptr: *mut IoRequestData,
    ) {
        // TODO https://github.com/tokio-rs/io-uring/issues/308
        // if self.is_supported(opcode::SendMsgZc::CODE) {
        //     self.register_entry(
        //         opcode::SendMsgZc::new(types::Fd(fd), msg_header).build(),
        //         request_ptr,
        //     );
        //     return;
        // }

        self.register_entry(
            opcode::SendMsg::new(types::Fd(fd), msg_header).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn peek(&mut self, fd: RawFd, buf_ptr: *mut u8, len: usize, request_ptr: *mut IoRequestData) {
        self.register_entry(
            #[allow(clippy::cast_possible_truncation, reason = "we have to cast it")]
            opcode::Recv::new(types::Fd(fd), buf_ptr, len as _)
                .flags(libc::MSG_PEEK)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn peek_from(
        &mut self,
        fd: RawFd,
        msg_header: &mut MessageRecvHeader,
        request_ptr: *mut IoRequestData,
    ) {
        let msg_header = &mut *msg_header;
        self.register_entry(
            opcode::RecvMsg::new(types::Fd(fd), &mut msg_header.header)
                .flags(libc::MSG_PEEK as u32)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn shutdown(&mut self, fd: RawFd, how: Shutdown, request_ptr: *mut IoRequestData) {
        let how = match how {
            Shutdown::Read => libc::SHUT_RD,
            Shutdown::Write => libc::SHUT_WR,
            Shutdown::Both => libc::SHUT_RDWR,
        };
        self.register_entry(
            opcode::Shutdown::new(types::Fd(fd), how).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn open(
        &mut self,
        path: *const libc::c_char,
        open_how: *const OpenHow,
        request_ptr: *mut IoRequestData,
    ) {
        self.register_entry(
            opcode::OpenAt2::new(types::Fd(libc::AT_FDCWD), path, open_how).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn fallocate(
        &mut self,
        fd: RawFd,
        offset: u64,
        len: u64,
        flags: i32,
        request_ptr: *mut IoRequestData,
    ) {
        self.register_entry(
            opcode::Fallocate::new(types::Fd(fd), len)
                .offset(offset)
                .mode(flags)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn sync_all(&mut self, fd: RawFd, request_ptr: *mut IoRequestData) {
        self.register_entry(opcode::Fsync::new(types::Fd(fd)).build(), request_ptr);
    }

    #[inline(always)]
    fn sync_data(&mut self, fd: RawFd, request_ptr: *mut IoRequestData) {
        self.register_entry(
            opcode::Fsync::new(types::Fd(fd))
                .flags(types::FsyncFlags::DATASYNC)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn read(&mut self, fd: RawFd, buf_ptr: *mut u8, len: usize, request_ptr: *mut IoRequestData) {
        self.register_entry(
            #[allow(clippy::cast_possible_truncation, reason = "we have to cast it")]
            opcode::Read::new(types::Fd(fd), buf_ptr, len as _).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn pread(
        &mut self,
        fd: RawFd,
        buf_ptr: *mut u8,
        len: usize,
        offset: usize,
        request_ptr: *mut IoRequestData,
    ) {
        self.register_entry(
            #[allow(clippy::cast_possible_truncation, reason = "we have to cast it")]
            opcode::Read::new(types::Fd(fd), buf_ptr, len as _)
                .offset(offset as _)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn write(
        &mut self,
        fd: RawFd,
        buf_ptr: *const u8,
        len: usize,
        request_ptr: *mut IoRequestData,
    ) {
        self.register_entry(
            #[allow(clippy::cast_possible_truncation, reason = "we have to cast it")]
            opcode::Write::new(types::Fd(fd), buf_ptr, len as _).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn pwrite(
        &mut self,
        fd: RawFd,
        buf_ptr: *const u8,
        len: usize,
        offset: usize,
        request_ptr: *mut IoRequestData,
    ) {
        self.register_entry(
            #[allow(clippy::cast_possible_truncation, reason = "we have to cast it")]
            opcode::Write::new(types::Fd(fd), buf_ptr, len as _)
                .offset(offset as _)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn close(&mut self, fd: RawFd, request_ptr: *mut IoRequestData) {
        self.register_entry(opcode::Close::new(types::Fd(fd)).build(), request_ptr);
    }

    #[inline(always)]
    fn rename(
        &mut self,
        old_path: *const libc::c_char,
        new_path: *const libc::c_char,
        request_ptr: *mut IoRequestData,
    ) {
        self.register_entry(
            opcode::RenameAt::new(
                types::Fd(libc::AT_FDCWD),
                old_path,
                types::Fd(libc::AT_FDCWD),
                new_path,
            )
            .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn create_dir(
        &mut self,
        path: *const libc::c_char,
        mode: u32,
        request_ptr: *mut IoRequestData,
    ) {
        self.register_entry(
            opcode::MkDirAt::new(types::Fd(libc::AT_FDCWD), path)
                .mode(mode)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn remove_file(&mut self, path: *const libc::c_char, request_ptr: *mut IoRequestData) {
        self.register_entry(
            opcode::UnlinkAt::new(types::Fd(libc::AT_FDCWD), path).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn remove_dir(&mut self, path: *const libc::c_char, request_ptr: *mut IoRequestData) {
        self.register_entry(
            opcode::UnlinkAt::new(types::Fd(libc::AT_FDCWD), path)
                .flags(libc::AT_REMOVEDIR)
                .build(),
            request_ptr,
        );
    }
}
