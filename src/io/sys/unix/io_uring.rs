use std::cell::UnsafeCell;
use std::collections::{BTreeSet, VecDeque};
use std::intrinsics::{unlikely};
use std::io::{Error, ErrorKind};
use std::net::Shutdown;
use std::time::{Instant};
use io_uring::{cqueue, IoUring, opcode, types};
use io_uring::squeue::Entry;
use io_uring::types::{OpenHow, SubmitArgs, Timespec};
use nix::libc;
use nix::libc::sockaddr;
use crate::Executor;
use crate::io::io_request::IoRequest;
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::{Fd, OsMessageHeader};
use crate::io::worker::{IoWorker};

const TIMEOUT: Timespec = Timespec::new().nsec(500_000);

pub(crate) struct IoUringWorker {
    timeout: SubmitArgs<'static, 'static>,
    /// # Why we need some cell?
    ///
    /// We can't rewrite engine ([`Selector`] trait) to use separately `ring` field and other fields in different methods.
    /// For example, we can't use `&mut self` in [`Scheduler::handle_coroutine_state`] function, because we are borrowing the `ring` before [`Scheduler::handle_coroutine_state`].
    /// So we need some cell not to destroy the abstraction.
    ///
    /// # Why we use UnsafeCell?
    ///
    /// Because we can guarantee that:
    /// * only one thread can borrow the [`IoUringWorker`] at the same time
    /// * only in the [`poll`] method we borrow the `ring` field for [`CompletionQueue`] and [`SubmissionQueue`],
    /// but only after the [`SubmissionQueue`] is submitted we start using the [`CompletionQueue`] that can call the [`IoUringWorker::push_sqe`]
    /// but it is safe, because the [`SubmissionQueue`] has already been read and submitted.
    ring: UnsafeCell<IoUring<Entry, cqueue::Entry>>,
    backlog: VecDeque<Entry>,
    time_bounded_io_task_queue: BTreeSet<TimeBoundedIoTask>
}

impl IoUringWorker {
    pub fn new() -> Self {
        Self {
            timeout: SubmitArgs::new().timespec(&TIMEOUT),
            // TODO cfg entries
            ring: UnsafeCell::new(IoUring::new(1024).unwrap()),
            backlog: VecDeque::with_capacity(64),
            time_bounded_io_task_queue: BTreeSet::new()
        }
    }

    #[inline(always)]
    fn add_sqe(&mut self, sqe: Entry) {
        let ring = unsafe { &mut *self.ring.get() };
        unsafe {
            if ring.submission().push(&sqe).is_err() {
                self.backlog.push_back(sqe);
            }
        }
    }

    #[inline(always)]
    fn register_entry_with_u64_data(&mut self, sqe: Entry, data: u64) {
        let sqe = sqe.user_data(data);
        self.add_sqe(sqe);
    }

    #[inline(always)]
    fn register_entry(&mut self, sqe: Entry, data: *const IoRequest) {
        self.register_entry_with_u64_data(sqe, data as _);
    }

    #[inline(always)]
    fn cancel_entry(&mut self, data: u64) {
        self.add_sqe(opcode::AsyncCancel::new(data).build());
    }

    #[inline(always)]
    fn check_deadlines(&mut self) {
        let now = Instant::now();

        while let Some(time_bounded_io_task) = self.time_bounded_io_task_queue.pop_first() {
            if time_bounded_io_task.deadline() <= now {
                self.cancel_entry(time_bounded_io_task.user_data());
            } else {
                self.time_bounded_io_task_queue.insert(time_bounded_io_task.clone());
                break;
            }
        }
    }

    #[inline(always)]
    fn submit(&mut self) -> Result<(), Error> {
        let ring = unsafe { &mut *self.ring.get() };
        let mut sq = unsafe { ring.submission_shared() };
        let submitter = ring.submitter();

        loop {
            if sq.is_full() {
                match submitter.submit() {
                    Ok(_) => (),
                    Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => break,
                    Err(err) => return Err(err.into()),
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

        match submitter.submit_with_args(1, &self.timeout) {
            Ok(_) => (),
            Err(ref err) if err.raw_os_error() == Some(libc::ETIME) => (),
            Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => (),
            Err(err) => return Err(err.into()),
        };

        Ok(())
    }
}

// TODO opcode::ProvideBuffers. Read tokio-uring::io::pool for more information

impl IoWorker for IoUringWorker {
    #[inline(always)]
    fn register_time_bounded_io_task(&mut self, time_bounded_io_task: &mut TimeBoundedIoTask) {
        while unlikely(!self.time_bounded_io_task_queue.insert(time_bounded_io_task.clone())) {
            time_bounded_io_task.inc_deadline();
        }
    }

    #[inline(always)]
    fn deregister_time_bounded_io_task(&mut self, time_bounded_io_task: &TimeBoundedIoTask) {
        self.time_bounded_io_task_queue.remove(time_bounded_io_task);
    }

    #[inline(always)]
    fn must_poll(&mut self) {
        self.check_deadlines();
        if let Err(err) = self.submit() {
            panic!("IoUringWorker::submit() failed, err: {:?}", err);
        }

        let ring = unsafe { &mut *self.ring.get() };
        let mut cq = ring.completion();
        cq.sync();

        for cqe in &mut cq {
            if unlikely(cqe.user_data() == 0) {
                // AsyncCancel was done.
                continue;
            }

            let ret = cqe.result();
            let io_request_ptr = cqe.user_data() as *mut IoRequest;
            let io_request = unsafe { &mut *io_request_ptr };

            if unlikely(ret < 0) {
                if unlikely(ret == -libc::ECANCELED) {
                    io_request.set_ret(Err(Error::from(ErrorKind::TimedOut)));
                } else {
                    io_request.set_ret(Err(Error::from_raw_os_error(-ret)));
                }
            } else {
                io_request.set_ret(Ok(ret as _));
            }

            Executor::exec_task(io_request.task());
        }
    }

    #[inline(always)]
    fn accept(&mut self, listen_fd: Fd, addr: *mut sockaddr, addrlen: *mut libc::socklen_t, request_ptr: *const IoRequest) {
        self.register_entry(opcode::Accept::new(types::Fd(listen_fd), addr, addrlen).build(), request_ptr);
    }
    
    #[inline(always)]
    fn connect(&mut self, socket_fd: Fd, addr_ptr: *const sockaddr, addr_len: libc::socklen_t, request_ptr: *const IoRequest) {
        self.register_entry(opcode::Connect::new(types::Fd(socket_fd), addr_ptr, addr_len).build(), request_ptr);
    }
    
    #[inline(always)]
    fn poll_fd_read(&mut self, fd: Fd, request_ptr: *const IoRequest) {
        self.register_entry(opcode::PollAdd::new(types::Fd(fd), libc::POLLIN as _).build(), request_ptr);
    }

    #[inline(always)]
    fn poll_fd_write(&mut self, fd: Fd, request_ptr: *const IoRequest) {
        self.register_entry(opcode::PollAdd::new(types::Fd(fd), libc::POLLOUT as _).build(), request_ptr);
    }
    
    #[inline(always)]
    fn recv(&mut self, fd: Fd, buf_ptr: *mut u8, len: usize, request_ptr: *const IoRequest) {
        self.register_entry(opcode::Recv::new(types::Fd(fd), buf_ptr, len as _).build(), request_ptr);
    }

    #[inline(always)]
    fn recv_from(&mut self, fd: Fd, msg_header: *mut OsMessageHeader, request_ptr: *const IoRequest) {
        self.register_entry(opcode::RecvMsg::new(types::Fd(fd), msg_header).build(), request_ptr);
    }

    #[inline(always)]
    fn peek(&mut self, fd: Fd, buf_ptr: *mut u8, len: usize, request_ptr: *const IoRequest) {
        self.register_entry(
            opcode::Recv::new(types::Fd(fd), buf_ptr, len as _)
                .flags(libc::MSG_PEEK)
                .build(),
            request_ptr
        );
    }

    #[inline(always)]
    fn peek_from(&mut self, fd: Fd, msg_header: *mut OsMessageHeader, request_ptr: *const IoRequest) {
        let msg_header = unsafe { &mut *msg_header };
        self.register_entry(opcode::RecvMsg::new(types::Fd(fd), msg_header).flags(libc::MSG_PEEK as u32).build(), request_ptr);
    }
    
    #[inline(always)]
    fn send(&mut self, fd: Fd, buf_ptr: *const u8, len: usize, request_ptr: *const IoRequest) {
        self.register_entry(opcode::Send::new(types::Fd(fd), buf_ptr, len as _).build(), request_ptr);
    }

    #[inline(always)]
    fn send_to(&mut self, fd: Fd, msg_header: *const OsMessageHeader, request_ptr: *const IoRequest) {
        self.register_entry(opcode::SendMsg::new(types::Fd(fd), msg_header).build(), request_ptr);
    }

    #[inline(always)]
    fn shutdown(&mut self, fd: Fd, how: Shutdown, request_ptr: *const IoRequest) {
        let how = match how {
            Shutdown::Read => libc::SHUT_RD,
            Shutdown::Write => libc::SHUT_WR,
            Shutdown::Both => libc::SHUT_RDWR,
        } ;
        self.register_entry(opcode::Shutdown::new(types::Fd(fd), how).build(), request_ptr);
    }
    
    #[inline(always)]
    fn open(&mut self, path: *const libc::c_char, open_how: *const OpenHow, request_ptr: *const IoRequest) {
        self.register_entry(opcode::OpenAt2::new(types::Fd(libc::AT_FDCWD), path, open_how).build(), request_ptr);
    }
    
    #[inline(always)]
    fn read(&mut self, fd: Fd, buf_ptr: *mut u8, len: usize, request_ptr: *const IoRequest) {
        self.register_entry(opcode::Read::new(types::Fd(fd), buf_ptr, len as _).build(), request_ptr);
    }
    
    #[inline(always)]
    fn pread(&mut self, fd: Fd, buf_ptr: *mut u8, len: usize, offset: usize, request_ptr: *const IoRequest) {
        self.register_entry(opcode::Read::new(types::Fd(fd), buf_ptr, len as _).offset(offset as _).build(), request_ptr);
    }
    
    #[inline(always)]
    fn write(&mut self, fd: Fd, buf_ptr: *const u8, len: usize, request_ptr: *const IoRequest) {
        self.register_entry(opcode::Write::new(types::Fd(fd), buf_ptr, len as _).build(), request_ptr);
    }
    
    #[inline(always)]
    fn pwrite(&mut self, fd: Fd, buf_ptr: *const u8, len: usize, offset: usize, request_ptr: *const IoRequest) {
        self.register_entry(opcode::Write::new(types::Fd(fd), buf_ptr, len as _).offset(offset as _).build(), request_ptr);
    }
    
    #[inline(always)]
    fn close(&mut self, fd: Fd, request_ptr: *const IoRequest) {
        self.register_entry(opcode::Close::new(types::Fd(fd)).build(), request_ptr);
    }
    
    #[inline(always)]
    fn rename(&mut self, old_path: *const libc::c_char, new_path: *const libc::c_char, request_ptr: *const IoRequest) {
        self.register_entry(opcode::RenameAt::new(types::Fd(libc::AT_FDCWD), old_path, types::Fd(libc::AT_FDCWD), new_path).build(), request_ptr);
    }
    
    #[inline(always)]
    fn create_dir(&mut self, path: *const libc::c_char, mode: u32, request_ptr: *const IoRequest) {
        self.register_entry(opcode::MkDirAt::new(types::Fd(libc::AT_FDCWD), path).mode(mode).build(), request_ptr);
    }
    
    #[inline(always)]
    fn remove_file(&mut self, path: *const libc::c_char, request_ptr: *const IoRequest) {
        self.register_entry(opcode::UnlinkAt::new(types::Fd(libc::AT_FDCWD), path).build(), request_ptr);
    }
    
    #[inline(always)]
    fn remove_dir(&mut self, path: *const libc::c_char, request_ptr: *const IoRequest) {
        self.register_entry(opcode::UnlinkAt::new(types::Fd(libc::AT_FDCWD), path).flags(libc::AT_REMOVEDIR).build(), request_ptr);
    }
}