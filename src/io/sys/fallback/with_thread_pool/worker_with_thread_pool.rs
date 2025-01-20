use crate::io::io_request_data::IoRequestDataPtr;
use crate::io::sys::fallback::io_call::IoCall;
use crate::io::sys::fallback::mio_poller::MioPoller;
use crate::io::sys::{
    MessageRecvHeader, OsMessageHeader, OsOpenOptions, OsPathPtr, RawFile, RawSocket,
};
use crate::io::time_bounded_io_task::TimeBoundedIoTask;
use crate::io::worker::IoWorker;
use crate::io::{sys, IoWorkerConfig};
use crate::local_executor;
use crate::runtime::call::Call;
use crate::runtime::Task;
use mio::Interest;
use socket2::{Domain, Protocol, Type};
use std::cell::UnsafeCell;
use std::collections::BTreeSet;
use std::net::Shutdown;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{io, mem, thread};

enum WorkerResult {
    Done(Task),
    MustPoll(IoCall, IoRequestDataPtr),
}

type SyncWorkerResultList = std::sync::Mutex<Vec<WorkerResult>>;

struct ThreadWorker {
    task_channel: crossbeam::channel::Receiver<(IoCall, IoRequestDataPtr)>,
    completions: Arc<SyncWorkerResultList>,
}

impl ThreadWorker {
    /// Creates a new instance of `ThreadWorker`.
    pub(crate) fn new(
        completions: Arc<SyncWorkerResultList>,
    ) -> (Self, crossbeam::channel::Sender<(IoCall, IoRequestDataPtr)>) {
        let (sender, receiver) = crossbeam::channel::unbounded();
        (
            Self {
                task_channel: receiver,
                completions,
            },
            sender,
        )
    }

    /// Runs the worker until the channel is closed.
    pub(crate) fn run(&self) -> Result<(), crossbeam::channel::RecvError> {
        loop {
            let (call, data_ptr) = self.task_channel.recv()?;
            let data = data_ptr.get_mut();

            loop {
                // loop for handle interrupted error
                let res_ = call.do_io_work();

                let completion = match res_ {
                    Ok(res) => {
                        data.set_ret(Ok(res));

                        WorkerResult::Done(unsafe { data.task() })
                    }

                    Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                        WorkerResult::MustPoll(call, data_ptr)
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
                                WorkerResult::MustPoll(call, data_ptr)
                            } else {
                                data.set_ret(Err(err));

                                WorkerResult::Done(unsafe { data.task() })
                            }
                        }

                        #[cfg(not(unix))]
                        {
                            data.set_ret(Err(err));

                            WorkerResult::Done(unsafe { data.task() })
                        }
                    }
                };

                self.completions.lock().unwrap().push(completion);

                break;
            }
        }
    }
}

/// A fallback implementation of [`IoWorker`] that uses a thread pool.
pub(crate) struct FallbackWorker {
    number_of_active_tasks: usize,
    workers: Box<[crossbeam::channel::Sender<(IoCall, IoRequestDataPtr)>]>,
    completions: Arc<SyncWorkerResultList>,
    /// Vec to swap with mutex-protected `completions`
    synced_completions: UnsafeCell<Vec<WorkerResult>>,
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
    /// Pushes a task to a worker pool of the `FallbackWorker`.
    #[inline]
    pub(crate) fn push_to_worker_pool(
        &mut self,
        io_call: IoCall,
        io_request_data: IoRequestDataPtr,
    ) {
        let worker = &self.workers[self.number_of_active_tasks % self.workers.len()];
        worker.send((io_call, io_request_data)).expect(
            "ThreadWorker is disconnected. It is only possible if the thread has panicked.",
        );

        self.number_of_active_tasks += 1;
    }

    /// Pushes a task to a worker pool of the `FallbackWorker` considering deadline.
    #[inline]
    pub(crate) fn push_to_worker_pool_with_deadline(
        &mut self,
        io_call: IoCall,
        io_request_data: IoRequestDataPtr,
    ) {
        debug_assert!(io_call.deadline().is_some());

        check_deadline_and!(self, io_call.deadline().unwrap(), io_request_data, {
            self.push_to_worker_pool(io_call, io_request_data);
        });
    }

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
        let deadline = slot.0.deadline_mut().unwrap();
        let raw_socket = slot.0.raw_socket().unwrap();

        let mut time_bounded_io_task =
            TimeBoundedIoTask::new(slot.1, *deadline, raw_socket, slot_ptr);
        while !self.time_bounded_io_task_queue.insert(time_bounded_io_task) {
            *deadline += Duration::from_nanos(1);
            time_bounded_io_task = TimeBoundedIoTask::new(slot.1, *deadline, raw_socket, slot_ptr);
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

    /// Schedules a non-retriable IO call. It also decreases the number of active tasks.
    #[allow(unused_variables, reason = "We use #[cfg(debug_assertions)] here.")]
    fn schedule_io_call_that_not_retriable(
        &mut self,
        io_call: &IoCall,
        io_request_data_ptr: IoRequestDataPtr,
    ) {
        debug_assert!(!io_call.must_retry());

        self.number_of_active_tasks -= 1;

        let io_request_data = io_request_data_ptr.get_mut();
        let task = unsafe { io_request_data.task() };

        io_request_data.set_ret(Ok(0)); // This IO-Call is just a poll. So, it don't depend on the return value.

        if task.is_local() {
            local_executor().exec_task(task);
        } else {
            local_executor().spawn_shared_task(task);
        }
    }

    /// Sync `completions` and returns a mutable reference to it.
    fn synced_completions(&mut self) -> &'static mut Vec<WorkerResult> {
        {
            let mut completions = self
                .completions
                .lock()
                .expect("Failed to lock completions. Maybe Orengine doesn't support current OS.");

            mem::swap(&mut *completions, self.synced_completions.get_mut());
        };

        unsafe { &mut *self.synced_completions.get() }
    }

    /// Polls and processes all the polled requests. Returns if this function have done io work.  
    fn poll_and_process(&mut self, timeout: Duration) -> bool {
        let mut have_done = false;

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
                            self.push_to_worker_pool(io_call, io_request_data_ptr);
                        } else {
                            have_done = true;
                            self.schedule_io_call_that_not_retriable(&io_call, io_request_data_ptr);
                        }
                    });
                } else if io_call.must_retry() {
                    self.push_to_worker_pool(io_call, io_request_data_ptr);
                } else {
                    have_done = true;
                    self.schedule_io_call_that_not_retriable(&io_call, io_request_data_ptr);
                }
            } else {
                if let Some(deadline) = io_call.deadline() {
                    assert!(
                        self.time_bounded_io_task_queue.remove(&deadline),
                        "Failed to remove time bounded io task in an error case."
                    );
                }

                have_done = true;
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

        unsafe {
            self.polled_requests.set_len(0);
        }

        have_done
    }

    /// Returns if this function have done io work.
    #[must_use]
    fn must_poll_(&mut self, timeout: Duration) -> bool {
        let mut have_done = self.poll_and_process(timeout);

        self.check_deadlines();

        let completions = self.synced_completions();

        self.number_of_active_tasks -= completions.len();

        for result in completions.drain(..) {
            match result {
                WorkerResult::Done(task) => {
                    have_done = true;

                    if task.is_local() {
                        local_executor().exec_task(task);
                    } else {
                        local_executor().spawn_shared_task(task);
                    }
                }
                WorkerResult::MustPoll(io_call, io_request_data) => {
                    debug_assert!(
                        io_call.is_recv_pollable()
                            || io_call.is_send_pollable() | io_call.is_both_pollable()
                    );

                    self.number_of_active_tasks += 1;

                    let interest = if io_call.is_recv_pollable() {
                        Interest::READABLE
                    } else if io_call.is_send_pollable() {
                        Interest::WRITABLE
                    } else if io_call.is_both_pollable() {
                        Interest::WRITABLE | Interest::READABLE
                    } else {
                        unreachable!()
                    };

                    if io_call.deadline().is_some() {
                        let slot_ptr = self.poller.register(interest, (io_call, io_request_data));

                        self.register_deadline(slot_ptr);
                    } else {
                        self.poller.register(interest, (io_call, io_request_data));
                    }
                }
            }
        }

        have_done
    }
}

impl IoWorker for FallbackWorker {
    fn new(config: IoWorkerConfig) -> Self {
        let mut workers =
            Vec::with_capacity(config.fallback.number_of_threads_per_executor as usize);
        let completions = Arc::new(SyncWorkerResultList::new(Vec::with_capacity(16)));
        for _ in 0..config.fallback.number_of_threads_per_executor {
            let (worker, sender) = ThreadWorker::new(completions.clone());

            thread::spawn(move || {
                let _ = worker.run(); // RecvError means that FallbackWorker is stopped. It is fine.
            });

            workers.push(sender);
        }

        Self {
            number_of_active_tasks: 0,
            workers: workers.into_boxed_slice(),
            completions,
            synced_completions: UnsafeCell::new(Vec::new()),
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

        let mut timeout = timeout_option.unwrap();
        let mut diff = Duration::from_micros(16);

        if !self.must_poll_(timeout.min(diff)) {
            timeout = timeout.checked_sub(diff).unwrap_or(Duration::from_nanos(0));

            while timeout > Duration::from_nanos(0) {
                if self.must_poll_(timeout.min(diff)) {
                    break;
                }

                timeout = timeout.checked_sub(diff).unwrap_or(Duration::from_nanos(0));
                diff *= 2;
                if diff > Duration::from_millis(10) {
                    diff = Duration::from_millis(10);
                }
            }
        }
    }

    #[inline]
    fn socket(
        &mut self,
        domain: Domain,
        sock_type: Type,
        protocol: Protocol,
        request_ptr: IoRequestDataPtr,
    ) {
        self.push_to_worker_pool(IoCall::Socket(domain, sock_type, protocol), request_ptr);
    }

    #[inline]
    fn accept(
        &mut self,
        raw_socket: RawSocket,
        addr_ptr: *mut sys::os_sockaddr,
        addr_len: *mut sys::socklen_t,
        request_ptr: IoRequestDataPtr,
    ) {
        self.push_to_worker_pool(IoCall::Accept(raw_socket, addr_ptr, addr_len), request_ptr);
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
        self.push_to_worker_pool_with_deadline(
            IoCall::AcceptWithDeadline(raw_socket, addr_ptr, addr_len, deadline),
            request_ptr,
        );
    }

    #[inline]
    fn connect(
        &mut self,
        raw_socket: RawSocket,
        addr_ptr: *const sys::os_sockaddr,
        addr_len: sys::socklen_t,
        request_ptr: IoRequestDataPtr,
    ) {
        self.push_to_worker_pool(IoCall::Connect(raw_socket, addr_ptr, addr_len), request_ptr);
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
        self.push_to_worker_pool_with_deadline(
            IoCall::ConnectWithDeadline(raw_socket, addr_ptr, addr_len, deadline),
            request_ptr,
        );
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
        self.push_to_worker_pool(IoCall::Recv(raw_socket, ptr, len), request_ptr);
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
        self.push_to_worker_pool(IoCall::Recv(raw_socket, ptr, len), request_ptr);
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
        self.push_to_worker_pool_with_deadline(
            IoCall::RecvWithDeadline(raw_socket, ptr, len, deadline),
            request_ptr,
        );
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
        self.push_to_worker_pool_with_deadline(
            IoCall::RecvWithDeadline(raw_socket, ptr, len, deadline),
            request_ptr,
        );
    }

    #[inline]
    fn recv_from(
        &mut self,
        raw_socket: RawSocket,
        msg_header: &mut MessageRecvHeader,
        request_ptr: IoRequestDataPtr,
    ) {
        self.push_to_worker_pool(IoCall::RecvFrom(raw_socket, msg_header), request_ptr);
    }

    #[inline]
    fn recv_from_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        msg_header: &mut MessageRecvHeader,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        self.push_to_worker_pool_with_deadline(
            IoCall::RecvFromWithDeadline(raw_socket, msg_header, deadline),
            request_ptr,
        );
    }

    #[inline]
    fn send(
        &mut self,
        raw_socket: RawSocket,
        ptr: *const u8,
        len: u32,
        request_ptr: IoRequestDataPtr,
    ) {
        self.push_to_worker_pool(IoCall::Send(raw_socket, ptr, len), request_ptr);
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
        self.push_to_worker_pool(IoCall::Send(raw_socket, ptr, len), request_ptr);
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
        self.push_to_worker_pool_with_deadline(
            IoCall::SendWithDeadline(raw_socket, ptr, len, deadline),
            request_ptr,
        );
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
        self.push_to_worker_pool_with_deadline(
            IoCall::SendWithDeadline(raw_socket, ptr, len, deadline),
            request_ptr,
        );
    }

    #[inline]
    fn send_to(
        &mut self,
        raw_socket: RawSocket,
        msg_header: *const OsMessageHeader,
        request_ptr: IoRequestDataPtr,
    ) {
        self.push_to_worker_pool(IoCall::SendTo(raw_socket, msg_header), request_ptr);
    }

    #[inline]
    fn send_to_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        msg_header: *const OsMessageHeader,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        self.push_to_worker_pool_with_deadline(
            IoCall::SendToWithDeadline(raw_socket, msg_header, deadline),
            request_ptr,
        );
    }

    #[inline]
    fn peek(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        request_ptr: IoRequestDataPtr,
    ) {
        self.push_to_worker_pool(IoCall::Peek(raw_socket, ptr, len), request_ptr);
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
        self.push_to_worker_pool(IoCall::Peek(raw_socket, ptr, len), request_ptr);
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
        self.push_to_worker_pool_with_deadline(
            IoCall::PeekWithDeadline(raw_socket, ptr, len, deadline),
            request_ptr,
        );
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
        self.push_to_worker_pool_with_deadline(
            IoCall::PeekWithDeadline(raw_socket, ptr, len, deadline),
            request_ptr,
        );
    }

    #[inline]
    fn peek_from(
        &mut self,
        raw_socket: RawSocket,
        msg: &mut MessageRecvHeader,
        request_ptr: IoRequestDataPtr,
    ) {
        self.push_to_worker_pool(IoCall::PeekFrom(raw_socket, msg), request_ptr);
    }

    #[inline]
    fn peek_from_with_deadline(
        &mut self,
        raw_socket: RawSocket,
        msg: &mut MessageRecvHeader,
        request_ptr: IoRequestDataPtr,
        deadline: &mut Instant,
    ) {
        self.push_to_worker_pool_with_deadline(
            IoCall::PeekFromWithDeadline(raw_socket, msg, deadline),
            request_ptr,
        );
    }

    #[inline]
    fn shutdown(&mut self, raw_socket: RawSocket, how: Shutdown, request_ptr: IoRequestDataPtr) {
        self.push_to_worker_pool(IoCall::Shutdown(raw_socket, how), request_ptr);
    }

    #[inline]
    fn open(
        &mut self,
        path: OsPathPtr,
        open_how: *const OsOpenOptions,
        request_ptr: IoRequestDataPtr,
    ) {
        self.push_to_worker_pool(IoCall::Open(path, open_how), request_ptr);
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
        self.push_to_worker_pool(IoCall::Fallocate, request_ptr);
    }

    #[inline]
    fn sync_all(&mut self, raw_file: RawFile, request_ptr: IoRequestDataPtr) {
        self.push_to_worker_pool(IoCall::FAllSync(raw_file), request_ptr);
    }

    #[inline]
    fn sync_data(&mut self, raw_file: RawFile, request_ptr: IoRequestDataPtr) {
        self.push_to_worker_pool(IoCall::FDataSync(raw_file), request_ptr);
    }

    #[inline]
    fn read(&mut self, raw_file: RawFile, ptr: *mut u8, len: u32, request_ptr: IoRequestDataPtr) {
        self.push_to_worker_pool(IoCall::Read(raw_file, ptr, len), request_ptr);
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
        self.push_to_worker_pool(IoCall::Read(raw_file, ptr, len), request_ptr);
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
        self.push_to_worker_pool(
            IoCall::PRead(raw_file, ptr, len, offset as u64),
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
        self.push_to_worker_pool(
            IoCall::PRead(raw_file, ptr, len, offset as u64),
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
        self.push_to_worker_pool(IoCall::Write(raw_file, ptr, len), request_ptr);
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
        self.push_to_worker_pool(IoCall::Write(raw_file, ptr, len), request_ptr);
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
        self.push_to_worker_pool(
            IoCall::PWrite(raw_file, ptr, len, offset as u64),
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
        self.push_to_worker_pool(
            IoCall::PWrite(raw_file, ptr, len, offset as u64),
            request_ptr,
        );
    }

    #[inline]
    fn close_file(&mut self, raw_file: RawFile, request_ptr: IoRequestDataPtr) {
        self.push_to_worker_pool(IoCall::CloseFile(raw_file), request_ptr);
    }

    #[inline]
    fn close_socket(&mut self, raw_socket: RawSocket, request_ptr: IoRequestDataPtr) {
        self.push_to_worker_pool(IoCall::CloseSocket(raw_socket), request_ptr);
    }

    #[inline]
    fn rename(&mut self, old_path: OsPathPtr, new_path: OsPathPtr, request_ptr: IoRequestDataPtr) {
        self.push_to_worker_pool(IoCall::Rename(old_path, new_path), request_ptr);
    }

    #[inline]
    fn create_dir(&mut self, path: OsPathPtr, mode: u32, request_ptr: IoRequestDataPtr) {
        self.push_to_worker_pool(IoCall::CreateDir(path, mode), request_ptr);
    }

    #[inline]
    fn remove_file(&mut self, path: OsPathPtr, request_ptr: IoRequestDataPtr) {
        self.push_to_worker_pool(IoCall::RemoveFile(path), request_ptr);
    }

    #[inline]
    fn remove_dir(&mut self, path: OsPathPtr, request_ptr: IoRequestDataPtr) {
        self.push_to_worker_pool(IoCall::RemoveDir(path), request_ptr);
    }
}
