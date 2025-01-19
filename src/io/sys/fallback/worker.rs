use crate::bug_message::BUG_MESSAGE;
use crate::io::io_request_data::IoRequestDataPtr;
use crate::io::sys::fallback::io_call::IoCall;
use crate::io::sys::fallback::mio_poller::MioPoller;
use crate::io::sys::fallback::operations::{
    close_file_op, close_socket_op, fsync_data_op, fsync_op, mkdir_op, open_op, read_at_op,
    read_op, rename_op, rmdir_op, shutdown_op, socket_op, unlink_op, write_at_op, write_op,
};
use crate::io::sys::{
    MessageRecvHeader, OsMessageHeader, OsOpenOptions, OsPathPtr, RawFile, RawSocket,
};
use crate::io::time_bounded_io_task::TimeBoundedIoTask;
use crate::io::worker::IoWorker;
use crate::io::{sys, IoWorkerConfig};
use crate::local_executor;
use crate::runtime::call::Call;
use mio::Interest;
use socket2::{Domain, Protocol, Type};
use std::collections::BTreeSet;
use std::io;
use std::net::Shutdown;
use std::time::{Duration, Instant};

/// A fallback implementation of [`IoWorker`] that uses a thread pool.
pub(crate) struct FallbackWorker {
    number_of_active_tasks: usize,
    poller: MioPoller,
    time_bounded_io_task_queue: BTreeSet<TimeBoundedIoTask>,
    last_gotten_time: Instant,
    polled_requests: Vec<(Result<(), ()>, IoCall, IoRequestDataPtr)>,
}

macro_rules! check_deadline_and {
    ($worker:expr, $deadline:expr, $request_ptr:expr, $op:block) => {
        if $worker.last_gotten_time < $deadline {
            $op
        } else {
            let io_request_data = $request_ptr.get_mut();
            io_request_data.set_ret(Err(io::Error::from(io::ErrorKind::TimedOut)));

            let task = unsafe { io_request_data.task() };
            // We can't execute the task immediately, because
            // it is running now. But we can spawn it to execute it after Poll::Pending.
            // It's not often that a deadline has already passed, so it's not a performance issue.
            if task.is_local() {
                local_executor().spawn_local_task(task);
            } else {
                // We can't spawn shared task when it is running.
                unsafe {
                    local_executor().invoke_call(Call::PushCurrentTaskAtTheStartOfLIFOSharedQueue)
                }
            }
        }
    };
}

impl FallbackWorker {
    /// Registers a new [`TimeBoundedIoTask`] for a given socket.
    ///
    /// Logic of deadline:
    /// 1. deadline can be only on socket recv / peek / send / accept / connect operations
    /// 2. before execute the operation, the deadline will be checked
    /// 3. if deadline is expired, the operation will be cancelled immediately
    /// 4. else, we try to execute the operation
    /// 5. if not would block, return the result
    /// 6. else register deadline and register the socket to the poller
    /// 7. when the deadline expires, the socket will be deregistered from the poller
    ///    and `ErrorKind::TimedOut` will be returned
    /// 8. else if the socket is ready before the deadline expires, the operation will be executed
    ///    the result will be returned and the deadline will be deregistered
    pub(crate) fn register_deadline(&mut self, slot_ptr: *mut (IoCall, IoRequestDataPtr)) {
        let slot = unsafe { &mut *slot_ptr };
        let io_request_data = slot.1.get_mut();
        let deadline = slot.0.deadline_mut().unwrap();
        let raw_socket = slot.0.raw_socket().unwrap();

        let mut time_bounded_io_task =
            TimeBoundedIoTask::new(io_request_data, *deadline, raw_socket, slot_ptr);
        while !self.time_bounded_io_task_queue.insert(time_bounded_io_task) {
            *deadline += Duration::from_nanos(1);
            time_bounded_io_task =
                TimeBoundedIoTask::new(io_request_data, *deadline, raw_socket, slot_ptr);
        }
    }

    /// Checks for timed out requests and removes them from the queue.
    ///
    /// It saves the timed out requests to the provided vector.
    ///
    /// Read logic in [`Self::check_deadlines`].
    pub(crate) fn check_deadlines(&mut self) {
        self.last_gotten_time = Instant::now();

        while let Some(time_bounded_io_task) = self.time_bounded_io_task_queue.pop_first() {
            if time_bounded_io_task.deadline() <= self.last_gotten_time {
                self.poller
                    .deregister(
                        time_bounded_io_task.raw_socket(),
                        time_bounded_io_task.slot_ptr(),
                    )
                    .unwrap();

                let io_request_ptr = IoRequestDataPtr::from_u64(time_bounded_io_task.user_data());
                let io_request_data = io_request_ptr.get_mut();
                let task = unsafe { io_request_data.task() };

                io_request_data.set_ret(Err(io::Error::from(io::ErrorKind::TimedOut)));

                if task.is_local() {
                    local_executor().exec_task(task);
                } else {
                    local_executor().spawn_shared_task(task);
                }

                self.number_of_active_tasks -= 1;
            } else {
                self.time_bounded_io_task_queue.insert(time_bounded_io_task);

                break;
            }
        }
    }

    /// Schedules a non-retriable IO call. It doesn't decrease the number of active tasks.
    #[allow(unused_variables, reason = "We use #[cfg(debug_assertions)] here.")]
    fn schedule_io_call_that_not_retriable(
        io_call: &IoCall,
        io_request_data_ptr: IoRequestDataPtr,
    ) {
        debug_assert!(!io_call.must_retry());

        let io_request_data = io_request_data_ptr.get_mut();
        let task = unsafe { io_request_data.task() };

        io_request_data.set_ret(Ok(0)); // This IO-Call is just a poll. So, it don't depend on the return value.

        if task.is_local() {
            local_executor().exec_task(task);
        } else {
            local_executor().spawn_shared_task(task);
        }
    }

    /// Polls and processes all the polled requests. Returns if this function have done io work.  
    fn poll_and_process(&mut self, timeout: Duration) {
        self.poller
            .poll(Some(timeout), &mut self.polled_requests)
            .unwrap();

        let polled_requests_len = self.polled_requests.len();
        for i in 0..polled_requests_len {
            let (result, io_call, io_request_data_ptr) =
                unsafe { std::ptr::read(self.polled_requests.get_unchecked(i)) };
            if result.is_ok() {
                if let Some(deadline) = io_call.deadline() {
                    assert!(
                        self.time_bounded_io_task_queue.remove(&deadline),
                        "Failed to remove time bounded io task."
                    );

                    check_deadline_and!(self, deadline, io_request_data_ptr, {
                        if io_call.must_retry() {
                            self.handle_io_call(io_call, io_request_data_ptr);
                        } else {
                            Self::schedule_io_call_that_not_retriable(
                                &io_call,
                                io_request_data_ptr,
                            );
                        }
                    });
                } else if io_call.must_retry() {
                    self.handle_io_call(io_call, io_request_data_ptr);
                } else {
                    Self::schedule_io_call_that_not_retriable(&io_call, io_request_data_ptr);
                }
            } else {
                if let Some(deadline) = io_call.deadline() {
                    assert!(
                        self.time_bounded_io_task_queue.remove(&deadline),
                        "Failed to remove time bounded io task in an error case."
                    );
                }

                self.number_of_active_tasks -= 1;

                let io_request_data = io_request_data_ptr.get_mut();
                let task = unsafe { io_request_data.task() };

                io_request_data.set_ret(Err(io::Error::from(io::ErrorKind::Other)));

                if task.is_local() {
                    local_executor().exec_task(task);
                } else {
                    local_executor().spawn_shared_task(task);
                }
            }
        }

        self.number_of_active_tasks -= polled_requests_len;

        unsafe {
            self.polled_requests.set_len(0);
        }
    }

    /// Executes an IO operation and processes the result.
    fn handle_io_operation<Op>(op: Op, io_request_data_ptr: IoRequestDataPtr)
    where
        Op: Fn() -> io::Result<usize>,
    {
        loop {
            let result = match op() {
                Ok(ret) => Ok(ret),

                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    panic!("{BUG_MESSAGE}: io operation is not pollable but returns WouldBlock. Should use a handle_io_call");
                }

                Err(err) if err.kind() == io::ErrorKind::Interrupted => {
                    continue;
                }

                Err(err) => {
                    #[cfg(unix)]
                    {
                        if err
                            .raw_os_error()
                            .is_some_and(|code| code == libc::EINPROGRESS)
                        {
                            panic!("{BUG_MESSAGE}: io operation is not pollable but returns InProgress. Should use a handle_io_call");
                        } else {
                            Err(err)
                        }
                    }

                    #[cfg(not(unix))]
                    Err(err)
                }
            };

            let io_request_data = io_request_data_ptr.get_mut();
            let task = unsafe { io_request_data.task() };

            io_request_data.set_ret(result);

            // We have executed the provided operation in the current task. So, it is running now.
            // Therefore, we can only spawn a task, not to execute it immediately.
            if task.is_local() {
                local_executor().spawn_local_task(task);
            } else {
                // We can't spawn shared task when it is running.
                unsafe {
                    local_executor().invoke_call(Call::PushCurrentTaskAtTheStartOfLIFOSharedQueue);
                }
            }

            break;
        }
    }

    /// Registers an associated with [`IoCall`] socket to the poller.
    ///
    /// It also:
    ///
    /// - considers deadline;
    ///
    /// - increases the number of active tasks.
    fn register_io_call_to_poller_with_considering_deadline(
        &mut self,
        io_call: IoCall,
        io_request_data_ptr: IoRequestDataPtr,
    ) {
        self.number_of_active_tasks += 1;

        let interest = if io_call.is_recv_pollable() {
            Interest::READABLE
        } else if io_call.is_send_pollable() {
            Interest::WRITABLE
        } else if io_call.is_both_pollable() {
            Interest::READABLE | Interest::WRITABLE
        } else {
            unreachable!();
        };

        if io_call.deadline().is_some() {
            let slot_ptr = self
                .poller
                .register(interest, (io_call, io_request_data_ptr));

            self.register_deadline(slot_ptr);
        } else {
            self.poller
                .register(interest, (io_call, io_request_data_ptr));
        }
    }

    /// Executes an [`IoCall`] and processes the result.
    fn handle_io_call(&mut self, io_call: IoCall, io_request_data_ptr: IoRequestDataPtr) {
        loop {
            let result = match io_call.do_io_work() {
                Ok(ret) => Ok(ret),

                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    self.register_io_call_to_poller_with_considering_deadline(
                        io_call,
                        io_request_data_ptr,
                    );

                    return;
                }

                Err(err) if err.kind() == io::ErrorKind::Interrupted => {
                    continue;
                }

                Err(err) => {
                    #[cfg(unix)]
                    {
                        if err
                            .raw_os_error()
                            .is_some_and(|code| code == libc::EINPROGRESS)
                        {
                            self.register_io_call_to_poller_with_considering_deadline(
                                io_call,
                                io_request_data_ptr,
                            );

                            return;
                        }

                        Err(err)
                    }

                    #[cfg(not(unix))]
                    Err(err)
                }
            };

            let io_request_data = io_request_data_ptr.get_mut();
            let task = unsafe { io_request_data.task() };

            io_request_data.set_ret(result);

            // We have executed the provided operation in the current task. So, it is running now.
            // Therefore, we can only spawn a task, not to execute it immediately.
            if task.is_local() {
                local_executor().spawn_local_task(task);
            } else {
                // We can't spawn shared task when it is running.
                unsafe {
                    local_executor().invoke_call(Call::PushCurrentTaskAtTheStartOfLIFOSharedQueue);
                }
            }

            break;
        }
    }

    /// Returns if this function have done io work.
    fn must_poll_(&mut self, timeout: Duration) {
        self.poll_and_process(timeout);

        self.check_deadlines();
    }
}

impl IoWorker for FallbackWorker {
    fn new(_config: IoWorkerConfig) -> Self {
        Self {
            number_of_active_tasks: 0,
            poller: MioPoller::new().expect("Failed to create mio Poll instance."),
            time_bounded_io_task_queue: BTreeSet::new(),
            last_gotten_time: Instant::now(),
            polled_requests: Vec::new(),
        }
    }

    #[inline]
    fn deregister_time_bounded_io_task(&mut self, deadline: &Instant) {
        self.time_bounded_io_task_queue.remove(deadline);
    }

    #[inline]
    fn has_work(&self) -> bool {
        self.number_of_active_tasks > 0
    }

    #[inline]
    fn must_poll(&mut self, mut timeout_option: Option<Duration>) {
        if self.number_of_active_tasks == 0 {
            return;
        }

        if timeout_option.is_none() {
            timeout_option = Some(Duration::from_nanos(0));
        }

        self.must_poll_(timeout_option.unwrap());
    }

    fn socket(
        &mut self,
        domain: Domain,
        sock_type: Type,
        protocol: Protocol,
        request_ptr: IoRequestDataPtr,
    ) {
        Self::handle_io_operation(move || socket_op(domain, sock_type, protocol), request_ptr);
    }

    #[inline]
    fn accept(
        &mut self,
        raw_socket: RawSocket,
        addr_ptr: *mut sys::os_sockaddr,
        addr_len: *mut sys::socklen_t,
        request_ptr: IoRequestDataPtr,
    ) {
        self.handle_io_call(IoCall::Accept(raw_socket, addr_ptr, addr_len), request_ptr);
    }

    #[inline]
    fn accept_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        addr_ptr: *mut sys::os_sockaddr,
        addr_len: *mut sys::socklen_t,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        check_deadline_and!(self, *deadline, request_ptr, {
            self.handle_io_call(
                IoCall::AcceptWithDeadline(raw_socket, addr_ptr, addr_len, deadline),
                request_ptr,
            );
        });
    }

    #[inline]
    fn connect(
        &mut self,
        raw_socket: RawSocket,
        addr_ptr: *const sys::os_sockaddr,
        addr_len: sys::socklen_t,
        request_ptr: IoRequestDataPtr,
    ) {
        self.handle_io_call(IoCall::Connect(raw_socket, addr_ptr, addr_len), request_ptr);
    }

    #[inline]
    fn connect_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        addr_ptr: *const sys::os_sockaddr,
        addr_len: sys::socklen_t,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        check_deadline_and!(self, *deadline, request_ptr, {
            self.handle_io_call(
                IoCall::ConnectWithDeadline(raw_socket, addr_ptr, addr_len, deadline),
                request_ptr,
            );
        });
    }

    #[inline]
    fn poll_socket_read(&mut self, raw_socket: RawSocket, request_ptr: IoRequestDataPtr) {
        self.number_of_active_tasks += 1;
        self.poller.register(
            Interest::READABLE,
            (IoCall::PollRecv(raw_socket), request_ptr),
        );
    }

    #[inline]
    fn poll_socket_read_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        check_deadline_and!(self, *deadline, request_ptr, {
            self.number_of_active_tasks += 1;
            let slot_ptr = self.poller.register(
                Interest::READABLE,
                (
                    IoCall::PollRecvWithDeadline(raw_socket, deadline),
                    request_ptr,
                ),
            );
            self.register_deadline(slot_ptr);
        });
    }

    #[inline]
    fn poll_socket_write(&mut self, raw_socket: RawSocket, request_ptr: IoRequestDataPtr) {
        self.number_of_active_tasks += 1;
        self.poller.register(
            Interest::WRITABLE,
            (IoCall::PollSend(raw_socket), request_ptr),
        );
    }

    #[inline]
    fn poll_socket_write_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        check_deadline_and!(self, *deadline, request_ptr, {
            self.number_of_active_tasks += 1;
            let slot_ptr = self.poller.register(
                Interest::WRITABLE,
                (
                    IoCall::PollSendWithDeadline(raw_socket, deadline),
                    request_ptr,
                ),
            );
            self.register_deadline(slot_ptr);
        });
    }

    #[inline]
    fn recv(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        request_ptr: IoRequestDataPtr,
    ) {
        self.handle_io_call(IoCall::Recv(raw_socket, ptr, len), request_ptr);
    }

    #[inline]
    fn recv_fixed(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        _buf_index: u16,
        request_ptr: IoRequestDataPtr,
    ) {
        self.handle_io_call(IoCall::Recv(raw_socket, ptr, len), request_ptr);
    }

    #[inline]
    fn recv_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        check_deadline_and!(self, *deadline, request_ptr, {
            self.handle_io_call(
                IoCall::RecvWithDeadline(raw_socket, ptr, len, deadline),
                request_ptr,
            );
        });
    }

    #[inline]
    fn recv_fixed_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        _buf_index: u16,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        check_deadline_and!(self, *deadline, request_ptr, {
            self.handle_io_call(
                IoCall::RecvWithDeadline(raw_socket, ptr, len, deadline),
                request_ptr,
            );
        });
    }

    #[inline]
    fn recv_from(
        &mut self,
        raw_socket: RawSocket,
        msg_header: &mut MessageRecvHeader,
        request_ptr: IoRequestDataPtr,
    ) {
        self.handle_io_call(IoCall::RecvFrom(raw_socket, msg_header), request_ptr);
    }

    #[inline]
    fn recv_from_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        msg_header: &mut MessageRecvHeader,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        check_deadline_and!(self, *deadline, request_ptr, {
            self.handle_io_call(
                IoCall::RecvFromWithDeadline(raw_socket, msg_header, deadline),
                request_ptr,
            );
        });
    }

    #[inline]
    fn send(
        &mut self,
        raw_socket: RawSocket,
        ptr: *const u8,
        len: u32,
        request_ptr: IoRequestDataPtr,
    ) {
        self.handle_io_call(IoCall::Send(raw_socket, ptr, len), request_ptr);
    }

    #[inline]
    fn send_fixed(
        &mut self,
        raw_socket: RawSocket,
        ptr: *const u8,
        len: u32,
        _buf_index: u16,
        request_ptr: IoRequestDataPtr,
    ) {
        self.handle_io_call(IoCall::Send(raw_socket, ptr, len), request_ptr);
    }

    #[inline]
    fn send_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        ptr: *const u8,
        len: u32,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        check_deadline_and!(self, *deadline, request_ptr, {
            self.handle_io_call(
                IoCall::SendWithDeadline(raw_socket, ptr, len, deadline),
                request_ptr,
            );
        });
    }

    #[inline]
    fn send_fixed_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        ptr: *const u8,
        len: u32,
        _buf_index: u16,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        check_deadline_and!(self, *deadline, request_ptr, {
            self.handle_io_call(
                IoCall::SendWithDeadline(raw_socket, ptr, len, deadline),
                request_ptr,
            );
        });
    }

    #[inline]
    fn send_to(
        &mut self,
        raw_socket: RawSocket,
        msg_header: *const OsMessageHeader,
        request_ptr: IoRequestDataPtr,
    ) {
        self.handle_io_call(IoCall::SendTo(raw_socket, msg_header), request_ptr);
    }

    #[inline]
    fn send_to_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        msg_header: *const OsMessageHeader,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        check_deadline_and!(self, *deadline, request_ptr, {
            self.handle_io_call(
                IoCall::SendToWithDeadline(raw_socket, msg_header, deadline),
                request_ptr,
            );
        });
    }

    #[inline]
    fn peek(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        request_ptr: IoRequestDataPtr,
    ) {
        self.handle_io_call(IoCall::Peek(raw_socket, ptr, len), request_ptr);
    }

    #[inline]
    fn peek_fixed(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        _buf_index: u16,
        request_ptr: IoRequestDataPtr,
    ) {
        self.handle_io_call(IoCall::Peek(raw_socket, ptr, len), request_ptr);
    }

    #[inline]
    fn peek_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        check_deadline_and!(self, *deadline, request_ptr, {
            self.handle_io_call(
                IoCall::PeekWithDeadline(raw_socket, ptr, len, deadline),
                request_ptr,
            );
        });
    }

    #[inline]
    fn peek_fixed_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        _buf_index: u16,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        check_deadline_and!(self, *deadline, request_ptr, {
            self.handle_io_call(
                IoCall::PeekWithDeadline(raw_socket, ptr, len, deadline),
                request_ptr,
            );
        });
    }

    #[inline]
    fn peek_from(
        &mut self,
        raw_socket: RawSocket,
        msg: &mut MessageRecvHeader,
        request_ptr: IoRequestDataPtr,
    ) {
        self.handle_io_call(IoCall::PeekFrom(raw_socket, msg), request_ptr);
    }

    #[inline]
    fn peek_from_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        msg: &mut MessageRecvHeader,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        check_deadline_and!(self, *deadline, request_ptr, {
            self.handle_io_call(
                IoCall::PeekFromWithDeadline(raw_socket, msg, deadline),
                request_ptr,
            );
        });
    }

    #[inline]
    fn shutdown(&mut self, raw_socket: RawSocket, how: Shutdown, request_ptr: IoRequestDataPtr) {
        Self::handle_io_operation(move || shutdown_op(raw_socket, how), request_ptr);
    }

    #[inline]
    fn open(
        &mut self,
        path: OsPathPtr,
        open_how: *const OsOpenOptions,
        request_ptr: IoRequestDataPtr,
    ) {
        Self::handle_io_operation(move || open_op(path, open_how), request_ptr);
    }

    #[inline]
    fn fallocate(
        &mut self,
        _raw_file: RawFile,
        _offset: u64,
        _len: u64,
        _flags: i32,
        request_ptr: IoRequestDataPtr,
    ) {
        Self::handle_io_operation(move || Ok(0), request_ptr);
    }

    #[inline]
    fn sync_all(&mut self, raw_file: RawFile, request_ptr: IoRequestDataPtr) {
        Self::handle_io_operation(move || fsync_op(raw_file), request_ptr);
    }

    #[inline]
    fn sync_data(&mut self, raw_file: RawFile, request_ptr: IoRequestDataPtr) {
        Self::handle_io_operation(move || fsync_data_op(raw_file), request_ptr);
    }

    #[inline]
    fn read(&mut self, raw_file: RawFile, ptr: *mut u8, len: u32, request_ptr: IoRequestDataPtr) {
        Self::handle_io_operation(move || read_op(raw_file, ptr, len), request_ptr);
    }

    #[inline]
    fn read_fixed(
        &mut self,
        raw_file: RawFile,
        ptr: *mut u8,
        len: u32,
        _buf_index: u16,
        request_ptr: IoRequestDataPtr,
    ) {
        Self::handle_io_operation(move || read_op(raw_file, ptr, len), request_ptr);
    }

    #[inline]
    fn pread(
        &mut self,
        raw_file: RawFile,
        ptr: *mut u8,
        len: u32,
        offset: usize,
        request_ptr: IoRequestDataPtr,
    ) {
        Self::handle_io_operation(
            move || read_at_op(raw_file, offset as u64, ptr, len),
            request_ptr,
        );
    }

    #[inline]
    fn pread_fixed(
        &mut self,
        raw_file: RawFile,
        ptr: *mut u8,
        len: u32,
        _buf_index: u16,
        offset: usize,
        request_ptr: IoRequestDataPtr,
    ) {
        Self::handle_io_operation(
            move || read_at_op(raw_file, offset as u64, ptr, len),
            request_ptr,
        );
    }

    #[inline]
    fn write(
        &mut self,
        raw_file: RawFile,
        ptr: *const u8,
        len: u32,
        request_ptr: IoRequestDataPtr,
    ) {
        Self::handle_io_operation(move || write_op(raw_file, ptr, len), request_ptr);
    }

    #[inline]
    fn write_fixed(
        &mut self,
        raw_file: RawFile,
        ptr: *const u8,
        len: u32,
        _buf_index: u16,
        request_ptr: IoRequestDataPtr,
    ) {
        Self::handle_io_operation(move || write_op(raw_file, ptr, len), request_ptr);
    }

    #[inline]
    fn pwrite(
        &mut self,
        raw_file: RawFile,
        ptr: *const u8,
        len: u32,
        offset: usize,
        request_ptr: IoRequestDataPtr,
    ) {
        Self::handle_io_operation(
            move || write_at_op(raw_file, offset as u64, ptr, len),
            request_ptr,
        );
    }

    #[inline]
    fn pwrite_fixed(
        &mut self,
        raw_file: RawFile,
        ptr: *const u8,
        len: u32,
        _buf_index: u16,
        offset: usize,
        request_ptr: IoRequestDataPtr,
    ) {
        Self::handle_io_operation(
            move || write_at_op(raw_file, offset as u64, ptr, len),
            request_ptr,
        );
    }

    #[inline]
    fn close_file(&mut self, raw_file: RawFile, request_ptr: IoRequestDataPtr) {
        Self::handle_io_operation(
            move || {
                close_file_op(raw_file);
                Ok(0)
            },
            request_ptr,
        );
    }

    #[inline]
    fn close_socket(&mut self, raw_socket: RawSocket, request_ptr: IoRequestDataPtr) {
        Self::handle_io_operation(
            move || {
                close_socket_op(raw_socket);
                Ok(0)
            },
            request_ptr,
        );
    }

    #[inline]
    fn rename(&mut self, old_path: OsPathPtr, new_path: OsPathPtr, request_ptr: IoRequestDataPtr) {
        Self::handle_io_operation(move || rename_op(old_path, new_path), request_ptr);
    }

    #[inline]
    fn create_dir(&mut self, path: OsPathPtr, mode: u32, request_ptr: IoRequestDataPtr) {
        Self::handle_io_operation(move || mkdir_op(path, mode), request_ptr);
    }

    #[inline]
    fn remove_file(&mut self, path: OsPathPtr, request_ptr: IoRequestDataPtr) {
        Self::handle_io_operation(move || unlink_op(path), request_ptr);
    }

    #[inline]
    fn remove_dir(&mut self, path: OsPathPtr, request_ptr: IoRequestDataPtr) {
        Self::handle_io_operation(move || rmdir_op(path), request_ptr);
    }
}
