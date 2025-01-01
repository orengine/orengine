use crate::io::io_request_data::IoRequestData;
use crate::io::sys::fallback::io_call::IoCall;
use crate::io::sys::fallback::mio_poller::MioPoller;
use crate::io::sys::{MessageRecvHeader, OsMessageHeader, RawFile, RawSocket};
use crate::io::worker::IoWorker;
use crate::io::IoWorkerConfig;
use crate::runtime::Task;
use io_uring::types::OpenHow;
use nix::libc::{c_char, sockaddr, socklen_t};
use socket2::{Domain, Protocol, Type};
use std::net::Shutdown;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

enum WorkerResult {
    Done(Task),
    MustPoll(IoCall, IoRequestData),
}

type SyncWorkerResultList = std::sync::Mutex<Vec<WorkerResult>>;

struct ThreadWorker {
    task_channel: crossbeam::channel::Receiver<(IoCall, IoRequestData)>,
    completions: Arc<SyncWorkerResultList>,
}

impl ThreadWorker {
    /// Creates a new instance of `ThreadWorker`.
    pub(crate) fn new(
        completions: Arc<SyncWorkerResultList>,
    ) -> (Self, crossbeam::channel::Sender<(IoCall, IoRequestData)>) {
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
    pub(crate) fn run(&mut self) -> Result<(), crossbeam::channel::RecvError> {
        loop {
            let (call, data) = self.task_channel.recv()?;
        }
    }
}

pub(crate) struct FallbackWorker {
    number_of_active_tasks: usize,
    workers: Box<[crossbeam::channel::Sender<(IoCall, IoRequestData)>]>,
    completions: Arc<SyncWorkerResultList>,
    poller: MioPoller,
}

impl FallbackWorker {
    /// Creates a new instance of `FallbackWorker`.
    pub(crate) fn new(number_of_workers: usize) -> Self {
        let mut workers = Vec::with_capacity(number_of_workers);
        let completions = Arc::new(SyncWorkerResultList::new(Vec::with_capacity(16)));
        for _ in 0..number_of_workers {
            let (mut worker, sender) = ThreadWorker::new(completions.clone());

            thread::spawn(move || {
                let _ = worker.run(); // RecvError means that FallbackWorker is stopped. It is fine.
            });

            workers.push(sender);
        }

        Self {
            number_of_active_tasks: 0,
            workers: workers.into_boxed_slice(),
            completions,
            poller: MioPoller::new().expect("Failed to create mio Poll instance."),
        }
    }

    /// Pushes a task to a worker pool of the `FallbackWorker`.
    #[inline(always)]
    pub(crate) fn push_to_worker_pool(&mut self, io_call: IoCall, io_request_data: IoRequestData) {
        let worker = &self.workers[self.number_of_active_tasks % self.workers.len()];
        worker.send((io_call, io_request_data)).expect(
            "ThreadWorker is disconnected. It is only possible if the thread has panicked.",
        );

        self.number_of_active_tasks += 1;
    }

    /// Polls the `FallbackWorker` and returns whether the `FallbackWorker` has work to do.
    #[inline(always)]
    pub(crate) fn poll(&mut self) -> bool {
        if self.number_of_active_tasks == 0 {
            return false;
        }

        let mut completions = self
            .completions
            .lock()
            .expect("Failed to lock completions. Maybe Orengine doesn't support current OS.");
        self.number_of_active_tasks -= completions.len();

        for result in completions.drain(..) {
            match result {
                WorkerResult::Done(task) => if task.is_local() {},
                WorkerResult::MustPoll(io_call, io_request_data) => {
                    self.number_of_active_tasks += 1;
                }
            }
        }

        true
    }
}

impl IoWorker for FallbackWorker {
    fn new(config: IoWorkerConfig) -> Self {
        todo!()
    }

    fn register_time_bounded_io_task(
        &mut self,
        io_request_data: &IoRequestData,
        deadline: &mut Instant,
    ) {
        todo!()
    }

    fn deregister_time_bounded_io_task(&mut self, deadline: &Instant) {
        todo!()
    }

    fn has_work(&self) -> bool {
        todo!()
    }

    fn must_poll(&mut self, timeout_option: Option<Duration>) {
        todo!()
    }

    fn socket(
        &mut self,
        domain: Domain,
        sock_type: Type,
        protocol: Protocol,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn accept(
        &mut self,
        raw_socket: RawSocket,
        addr: *mut sockaddr,
        addrlen: *mut socklen_t,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn connect(
        &mut self,
        raw_socket: RawSocket,
        addr_ptr: *const sockaddr,
        addr_len: socklen_t,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn poll_socket_read(&mut self, raw_socket: RawSocket, request_ptr: *mut IoRequestData) {
        todo!()
    }

    fn poll_socket_write(&mut self, raw_socket: RawSocket, request_ptr: *mut IoRequestData) {
        todo!()
    }

    fn recv(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn recv_fixed(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        buf_index: u16,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn recv_from(
        &mut self,
        raw_socket: RawSocket,
        msg_header: &mut MessageRecvHeader,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn send(
        &mut self,
        raw_socket: RawSocket,
        ptr: *const u8,
        len: u32,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn send_fixed(
        &mut self,
        raw_socket: RawSocket,
        ptr: *const u8,
        len: u32,
        buf_index: u16,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn send_to(
        &mut self,
        raw_socket: RawSocket,
        msg_header: *const OsMessageHeader,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn peek(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn peek_fixed(
        &mut self,
        raw_socket: RawSocket,
        ptr: *mut u8,
        len: u32,
        buf_index: u16,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn peek_from(
        &mut self,
        raw_socket: RawSocket,
        msg: &mut MessageRecvHeader,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn shutdown(&mut self, raw_socket: RawSocket, how: Shutdown, request_ptr: *mut IoRequestData) {
        todo!()
    }

    fn open(
        &mut self,
        path: *const c_char,
        open_how: *const OpenHow,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn fallocate(
        &mut self,
        raw_file: RawFile,
        offset: u64,
        len: u64,
        flags: i32,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn sync_all(&mut self, raw_file: RawFile, request_ptr: *mut IoRequestData) {
        todo!()
    }

    fn sync_data(&mut self, raw_file: RawFile, request_ptr: *mut IoRequestData) {
        todo!()
    }

    fn read(&mut self, raw_file: RawFile, ptr: *mut u8, len: u32, request_ptr: *mut IoRequestData) {
        todo!()
    }

    fn read_fixed(
        &mut self,
        raw_file: RawFile,
        ptr: *mut u8,
        len: u32,
        buf_index: u16,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn pread(
        &mut self,
        raw_file: RawFile,
        ptr: *mut u8,
        len: u32,
        offset: usize,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn pread_fixed(
        &mut self,
        raw_file: RawFile,
        ptr: *mut u8,
        len: u32,
        buf_index: u16,
        offset: usize,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn write(
        &mut self,
        raw_file: RawFile,
        ptr: *const u8,
        len: u32,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn write_fixed(
        &mut self,
        raw_file: RawFile,
        ptr: *const u8,
        len: u32,
        buf_index: u16,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn pwrite(
        &mut self,
        raw_file: RawFile,
        ptr: *const u8,
        len: u32,
        offset: usize,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn pwrite_fixed(
        &mut self,
        raw_file: RawFile,
        ptr: *const u8,
        len: u32,
        buf_index: u16,
        offset: usize,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn close_file(&mut self, raw_file: RawFile, request_ptr: *mut IoRequestData) {
        todo!()
    }

    fn close_socket(&mut self, raw_socket: RawSocket, request_ptr: *mut IoRequestData) {
        todo!()
    }

    fn rename(
        &mut self,
        old_path: *const c_char,
        new_path: *const c_char,
        request_ptr: *mut IoRequestData,
    ) {
        todo!()
    }

    fn create_dir(&mut self, path: *const c_char, mode: u32, request_ptr: *mut IoRequestData) {
        todo!()
    }

    fn remove_file(&mut self, path: *const c_char, request_ptr: *mut IoRequestData) {
        todo!()
    }

    fn remove_dir(&mut self, path: *const c_char, request_ptr: *mut IoRequestData) {
        todo!()
    }
}
