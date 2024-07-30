use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::io::Result;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use io_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use crate::io::AsyncPollFd;
use crate::io::io_request::{IoRequest};
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::{AsFd, Fd, MessageHeader};
use crate::io::worker::{IoWorker, local_worker};
use crate::runtime::task::Task;

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct RecvFrom<'buf> {
    fd: Fd,
    msg_header: MessageHeader<'buf>,
    io_request: Option<IoRequest>
}

impl<'buf> RecvFrom<'buf> {
    #[inline(always)]
    pub fn new(fd: Fd, buf: &'buf mut [u8]) -> Self {
        let s = Self {
            fd,
            msg_header: MessageHeader::new_for_recv_from(buf),
            io_request: None
        };

        s
    }
}

impl<'buf> Future for RecvFrom<'buf> {
    type Output = Result<(usize, SocketAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
             worker.recv_from(this.fd, this.msg_header.get_os_message_header_ptr(), this.io_request.as_ref().unwrap_unchecked()),
             (ret, this.msg_header.socket_addr().as_socket().expect("Invalid socket address"))
        ));
    }
}

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct RecvFromWithDeadline<'buf> {
    fd: Fd,
    msg_header: MessageHeader<'buf>,
    time_bounded_io_task: TimeBoundedIoTask,
    io_request: Option<IoRequest>
}

impl<'buf> RecvFromWithDeadline<'buf> {
    #[inline(always)]
    pub fn new(fd: Fd, buf: &'buf mut [u8], deadline: Instant) -> Self {
        Self {
            fd,
            msg_header: MessageHeader::new_for_recv_from(buf),
            time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
            io_request: None
        }
    }
}

impl<'buf> Future for RecvFromWithDeadline<'buf> {
    type Output = Result<(usize, SocketAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_time_bounded_io_request!((
             worker.recv_from(this.fd, this.msg_header.get_os_message_header_ptr(), this.io_request.as_ref().unwrap_unchecked()),
             (ret, this.msg_header.socket_addr().as_socket().expect("Invalid socket address"))
        ));
    }
}

#[macro_export]
macro_rules! generate_recv_from {
    () => {
        #[inline(always)]
        pub fn recv_from<'a>(&mut self, buf: &'a mut [u8]) -> crate::io::RecvFrom<'a> {
            crate::io::RecvFrom::new(self.as_raw_fd(), buf)
        }

        #[inline(always)]
        pub fn recv_from_with_deadline<'a>(&mut self, buf: &'a mut [u8], deadline: Instant) -> crate::io::RecvFromWithDeadline<'a> {
            crate::io::RecvFromWithDeadline::new(self.as_raw_fd(), buf, deadline)
        }

        #[inline(always)]
        pub fn recv_from_with_timeout<'a>(&mut self, buf: &'a mut [u8], duration: Duration) -> crate::io::RecvFromWithDeadline<'a> {
            self.recv_from_with_deadline(buf, Instant::now() + duration)
        }
    };
}