use crate::io::config::IoWorkerConfig;
use crate::io::io_request_data::{IoRequestData, IoRequestDataPtr};
use crate::io::sys;
use crate::io::sys::{
    os_sockaddr, MessageRecvHeader, OsMessageHeader, OsPathPtr, RawFile, RawSocket,
};
use crate::io::time_bounded_io_task::TimeBoundedIoTask;
use crate::io::worker::IoWorker;
use crate::runtime::local_executor;
use crate::BUG_MESSAGE;
use io_uring::squeue::Entry;
use io_uring::types::{OpenHow, SubmitArgs, Timespec};
use io_uring::{cqueue, opcode, types, IoUring, Probe};
use libc;
use std::cell::UnsafeCell;
use std::collections::{BTreeSet, VecDeque};
use std::ffi::c_int;
use std::io::{Error, ErrorKind};
use std::net::Shutdown;
use std::ptr;
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

    /// Register __fixed__ buffers.
    pub(crate) fn register_buffers(&mut self, buffers: &[libc::iovec]) {
        let submitter = unsafe { &mut *self.ring.get() }.submitter();
        unsafe { submitter.register_buffers(buffers) }.expect(BUG_MESSAGE);
    }

    /// Deregister __fixed__ buffers.
    pub(crate) fn deregister_buffers(&mut self) {
        let submitter = unsafe { &mut *self.ring.get() }.submitter();
        submitter.unregister_buffers().expect(BUG_MESSAGE);
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
    fn register_entry(&mut self, sqe: Entry, data: IoRequestDataPtr) {
        self.register_entry_with_u64_data(sqe, data.as_u64());
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

    /// Registers a new time-bounded io task. It will be cancelled if the deadline is reached.
    ///
    /// It takes `&mut Instant` as a deadline because it increments the deadline by 1 nanosecond
    /// if it is not unique.
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

    /// Submits all accumulated requests and waits for completions or a timeout.
    #[inline(always)]
    fn submit_and_poll(&mut self, timeout_option: Option<Duration>) -> Result<(), Error> {
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

        let res = timeout_option.map_or_else(
            || submitter.submit(),
            |timeout| {
                #[allow(clippy::cast_possible_truncation, reason = "u32 is enough for 4 secs")]
                let timespec = Timespec::new().nsec(timeout.as_nanos() as u32);
                let args = SubmitArgs::new().timespec(&timespec);
                submitter.submit_with_args(1, &args)
            },
        );

        match res {
            Ok(_) => (),
            Err(ref err) if err.raw_os_error() == Some(libc::ETIME) => (),
            Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => (),
            Err(err) => return Err(err),
        }

        Ok(())
    }
}

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
    fn deregister_time_bounded_io_task(&mut self, deadline: &Instant) {
        self.time_bounded_io_task_queue.remove(deadline);
    }

    #[inline(always)]
    fn has_work(&self) -> bool {
        self.number_of_active_tasks > 0
    }

    #[inline(always)]
    fn must_poll(&mut self, timeout_option: Option<Duration>) {
        let executor = local_executor();
        self.check_deadlines();
        self.submit_and_poll(timeout_option)
            .expect("IOUringWorker::submit() failed");

        let ring = unsafe { &mut *self.ring.get() };
        let mut cq = ring.completion();
        cq.sync();

        self.number_of_active_tasks -= cq.len();
        for cqe in cq {
            if cqe.user_data() == ASYNC_CLOSE_DATA {
                continue;
            }

            let ret = cqe.result();
            let io_request_ptr = IoRequestDataPtr::from_u64(cqe.user_data());
            let io_request = io_request_ptr.get_mut();

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
    }

    #[inline(always)]
    fn socket(
        &mut self,
        domain: socket2::Domain,
        sock_type: socket2::Type,
        protocol: socket2::Protocol,
        request_ptr: IoRequestDataPtr,
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

        let request = request_ptr.get_mut();
        let socket_ = socket2::Socket::new(domain, sock_type, Some(protocol));
        #[allow(
            clippy::cast_sign_loss,
            reason = "the sing was checked by map() method"
        )]
        request.set_ret(socket_.map(|s| sys::IntoRawSocket::into_raw_socket(s) as usize));

        local_executor().spawn_local_task(unsafe { request.task() });
    }

    #[inline(always)]
    fn accept(
        &mut self,
        raw_socket: RawSocket,
        addr_ptr: *mut os_sockaddr,
        addr_len: *mut sys::socklen_t,
        request_ptr: IoRequestDataPtr,
    ) {
        self.register_entry(
            opcode::Accept::new(types::Fd(raw_socket), addr_ptr, addr_len).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn accept_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        addr_ptr: *mut os_sockaddr,
        addr_len: *mut sys::socklen_t,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        self.register_time_bounded_io_task(request_ptr.get_mut() as _, deadline);
        self.accept(raw_socket, addr_ptr, addr_len, request_ptr);
    }

    #[inline(always)]
    fn connect(
        &mut self,
        raw_socket: RawSocket,
        addr_ptr: *const os_sockaddr,
        addr_len: sys::socklen_t,
        request_ptr: IoRequestDataPtr,
    ) {
        self.register_entry(
            opcode::Connect::new(types::Fd(raw_socket), addr_ptr, addr_len).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn connect_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        addr_ptr: *const os_sockaddr,
        addr_len: sys::socklen_t,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        self.register_time_bounded_io_task(request_ptr.get_mut(), deadline);
        self.connect(raw_socket, addr_ptr, addr_len, request_ptr);
    }

    #[inline(always)]
    fn poll_socket_read(&mut self, raw_socket: RawSocket, request_ptr: IoRequestDataPtr) {
        self.register_entry(
            opcode::PollAdd::new(types::Fd(raw_socket), libc::POLLIN as _).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn poll_socket_read_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        self.register_time_bounded_io_task(request_ptr.get_mut(), deadline);
        self.poll_socket_read(raw_socket, request_ptr);
    }

    #[inline(always)]
    fn poll_socket_write(&mut self, raw_socket: RawSocket, request_ptr: IoRequestDataPtr) {
        self.register_entry(
            opcode::PollAdd::new(types::Fd(raw_socket), libc::POLLOUT as _).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn poll_socket_write_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        self.register_time_bounded_io_task(request_ptr.get_mut(), deadline);
        self.poll_socket_write(raw_socket, request_ptr);
    }

    #[inline(always)]
    fn recv(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        request_ptr: IoRequestDataPtr,
    ) {
        self.register_entry(
            opcode::Recv::new(types::Fd(raw_socket), ptr, len).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn recv_fixed(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        buf_index: u16,
        request_ptr: IoRequestDataPtr,
    ) {
        self.register_entry(
            opcode::ReadFixed::new(types::Fd(raw_socket), ptr, len, buf_index).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn recv_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        self.register_time_bounded_io_task(request_ptr.get_mut(), deadline);
        self.recv(raw_socket, ptr, len, request_ptr);
    }

    #[inline(always)]
    fn recv_fixed_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        buf_index: u16,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        self.register_time_bounded_io_task(request_ptr.get_mut(), deadline);
        self.recv_fixed(raw_socket, ptr, len, buf_index, request_ptr);
    }

    #[inline(always)]
    fn recv_from(
        &mut self,
        raw_socket: RawSocket,
        msg_header: &mut MessageRecvHeader,
        request_ptr: IoRequestDataPtr,
    ) {
        self.register_entry(
            opcode::RecvMsg::new(
                types::Fd(raw_socket),
                ptr::from_mut(msg_header.get_os_message_header()),
            )
            .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn recv_from_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        msg_header: &mut MessageRecvHeader,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        self.register_time_bounded_io_task(request_ptr.get_mut(), deadline);
        self.recv_from(raw_socket, msg_header, request_ptr);
    }

    #[inline(always)]
    fn send(
        &mut self,
        raw_socket: RawSocket,
        ptr: *const u8,
        len: u32,
        request_ptr: IoRequestDataPtr,
    ) {
        self.register_entry(
            opcode::Send::new(types::Fd(raw_socket), ptr, len).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn send_fixed(
        &mut self,
        raw_socket: RawSocket,
        ptr: *const u8,
        len: u32,
        buf_index: u16,
        request_ptr: IoRequestDataPtr,
    ) {
        self.register_entry(
            opcode::WriteFixed::new(types::Fd(raw_socket), ptr, len, buf_index).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn send_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        ptr: *const u8,
        len: u32,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        self.register_time_bounded_io_task(request_ptr.get_mut(), deadline);
        self.send(raw_socket, ptr, len, request_ptr);
    }

    #[inline(always)]
    fn send_fixed_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        ptr: *const u8,
        len: u32,
        buf_index: u16,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        self.register_time_bounded_io_task(request_ptr.get_mut(), deadline);
        self.send_fixed(raw_socket, ptr, len, buf_index, request_ptr);
    }

    #[inline(always)]
    fn send_to(
        &mut self,
        raw_socket: RawSocket,
        msg_header: *const OsMessageHeader,
        request_ptr: IoRequestDataPtr,
    ) {
        self.register_entry(
            opcode::SendMsg::new(types::Fd(raw_socket), msg_header).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn send_to_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        msg_header: *const OsMessageHeader,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        self.register_time_bounded_io_task(request_ptr.get_mut(), deadline);
        self.send_to(raw_socket, msg_header, request_ptr);
    }

    #[inline(always)]
    fn peek(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        request_ptr: IoRequestDataPtr,
    ) {
        self.register_entry(
            opcode::Recv::new(types::Fd(raw_socket), ptr, len)
                .flags(libc::MSG_PEEK)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn peek_fixed(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        _buf_index: u16,
        request_ptr: IoRequestDataPtr,
    ) {
        // TODO peek fixed
        // self.register_entry(
        //     opcode::ReadFixed::new(types::Fd(raw_socket), ptr, len, buf_index)
        //         .rw_flags(libc::MSG_PEEK)
        //         .build(),
        //     request_ptr,
        // );

        self.peek(raw_socket, ptr, len, request_ptr);
    }

    #[inline(always)]
    fn peek_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        self.register_time_bounded_io_task(request_ptr.get_mut(), deadline);
        self.peek(raw_socket, ptr, len, request_ptr);
    }

    #[inline(always)]
    fn peek_fixed_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        buf_index: u16,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        self.register_time_bounded_io_task(request_ptr.get_mut(), deadline);
        self.peek_fixed(raw_socket, ptr, len, buf_index, request_ptr);
    }

    #[inline(always)]
    fn peek_from(
        &mut self,
        raw_socket: RawSocket,
        msg_header: &mut MessageRecvHeader,
        request_ptr: IoRequestDataPtr,
    ) {
        let msg_header = &mut *msg_header;
        self.register_entry(
            opcode::RecvMsg::new(types::Fd(raw_socket), msg_header.get_os_message_header())
                .flags(libc::MSG_PEEK as u32)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn peek_from_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        msg: &mut MessageRecvHeader,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        self.register_time_bounded_io_task(request_ptr.get_mut(), deadline);
        self.peek_from(raw_socket, msg, request_ptr);
    }

    #[inline(always)]
    fn shutdown(&mut self, raw_socket: RawSocket, how: Shutdown, request_ptr: IoRequestDataPtr) {
        let how = match how {
            Shutdown::Read => libc::SHUT_RD,
            Shutdown::Write => libc::SHUT_WR,
            Shutdown::Both => libc::SHUT_RDWR,
        };
        self.register_entry(
            opcode::Shutdown::new(types::Fd(raw_socket), how).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn open(&mut self, path: OsPathPtr, open_how: *const OpenHow, request_ptr: IoRequestDataPtr) {
        self.register_entry(
            opcode::OpenAt2::new(types::Fd(libc::AT_FDCWD), path, open_how).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn fallocate(
        &mut self,
        raw_file: RawFile,
        offset: u64,
        len: u64,
        flags: i32,
        request_ptr: IoRequestDataPtr,
    ) {
        self.register_entry(
            opcode::Fallocate::new(types::Fd(raw_file), len)
                .offset(offset)
                .mode(flags)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn sync_all(&mut self, raw_file: RawFile, request_ptr: IoRequestDataPtr) {
        self.register_entry(opcode::Fsync::new(types::Fd(raw_file)).build(), request_ptr);
    }

    #[inline(always)]
    fn sync_data(&mut self, raw_file: RawFile, request_ptr: IoRequestDataPtr) {
        self.register_entry(
            opcode::Fsync::new(types::Fd(raw_file))
                .flags(types::FsyncFlags::DATASYNC)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn read(&mut self, raw_file: RawFile, ptr: *mut u8, len: u32, request_ptr: IoRequestDataPtr) {
        #[allow(clippy::cast_sign_loss, reason = "we have to cast it")]
        self.register_entry(
            opcode::Read::new(types::Fd(raw_file), ptr, len)
                .offset(-1 as _)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn read_fixed(
        &mut self,
        raw_file: RawFile,
        ptr: *mut u8,
        len: u32,
        buf_index: u16,
        request_ptr: IoRequestDataPtr,
    ) {
        #[allow(clippy::cast_sign_loss, reason = "we have to cast it")]
        self.register_entry(
            opcode::ReadFixed::new(types::Fd(raw_file), ptr, len, buf_index)
                .offset(-1 as _)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn pread(
        &mut self,
        raw_file: RawFile,
        ptr: *mut u8,
        len: u32,
        offset: usize,
        request_ptr: IoRequestDataPtr,
    ) {
        self.register_entry(
            opcode::Read::new(types::Fd(raw_file), ptr, len)
                .offset(offset as _)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn pread_fixed(
        &mut self,
        raw_file: RawFile,
        ptr: *mut u8,
        len: u32,
        buf_index: u16,
        offset: usize,
        request_ptr: IoRequestDataPtr,
    ) {
        #[allow(clippy::cast_sign_loss, reason = "we have to cast it")]
        self.register_entry(
            opcode::ReadFixed::new(types::Fd(raw_file), ptr, len, buf_index)
                .offset(offset as _)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn write(
        &mut self,
        raw_file: RawFile,
        ptr: *const u8,
        len: u32,
        request_ptr: IoRequestDataPtr,
    ) {
        #[allow(clippy::cast_sign_loss, reason = "we have to cast it")]
        self.register_entry(
            opcode::Write::new(types::Fd(raw_file), ptr, len)
                .offset(-1 as _)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn write_fixed(
        &mut self,
        raw_file: RawFile,
        ptr: *const u8,
        len: u32,
        buf_index: u16,
        request_ptr: IoRequestDataPtr,
    ) {
        #[allow(clippy::cast_sign_loss, reason = "we have to cast it")]
        self.register_entry(
            opcode::WriteFixed::new(types::Fd(raw_file), ptr, len, buf_index)
                .offset(-1 as _)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn pwrite(
        &mut self,
        raw_file: RawFile,
        ptr: *const u8,
        len: u32,
        offset: usize,
        request_ptr: IoRequestDataPtr,
    ) {
        self.register_entry(
            opcode::Write::new(types::Fd(raw_file), ptr, len)
                .offset(offset as _)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn pwrite_fixed(
        &mut self,
        raw_file: RawFile,
        ptr: *const u8,
        len: u32,
        buf_index: u16,
        offset: usize,
        request_ptr: IoRequestDataPtr,
    ) {
        #[allow(clippy::cast_sign_loss, reason = "we have to cast it")]
        self.register_entry(
            opcode::WriteFixed::new(types::Fd(raw_file), ptr, len, buf_index)
                .offset(offset as _)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn close_file(&mut self, raw_file: RawFile, request_ptr: IoRequestDataPtr) {
        self.register_entry(opcode::Close::new(types::Fd(raw_file)).build(), request_ptr);
    }

    #[inline(always)]
    fn close_socket(&mut self, raw_socket: RawSocket, request_ptr: IoRequestDataPtr) {
        self.register_entry(
            opcode::Close::new(types::Fd(raw_socket)).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn rename(&mut self, old_path: OsPathPtr, new_path: OsPathPtr, request_ptr: IoRequestDataPtr) {
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
    fn create_dir(&mut self, path: OsPathPtr, mode: u32, request_ptr: IoRequestDataPtr) {
        self.register_entry(
            opcode::MkDirAt::new(types::Fd(libc::AT_FDCWD), path)
                .mode(mode)
                .build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn remove_file(&mut self, path: OsPathPtr, request_ptr: IoRequestDataPtr) {
        self.register_entry(
            opcode::UnlinkAt::new(types::Fd(libc::AT_FDCWD), path).build(),
            request_ptr,
        );
    }

    #[inline(always)]
    fn remove_dir(&mut self, path: OsPathPtr, request_ptr: IoRequestDataPtr) {
        self.register_entry(
            opcode::UnlinkAt::new(types::Fd(libc::AT_FDCWD), path)
                .flags(libc::AT_REMOVEDIR)
                .build(),
            request_ptr,
        );
    }
}
