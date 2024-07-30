use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use io_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use crate::io::io_request::{IoRequest};
use crate::io::sys::{AsFd, Fd};
use crate::io::worker::{IoWorker, local_worker};
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::runtime::task::Task;

macro_rules! generate_poll {
    ($name: ident, $name_with_deadline: ident, $method: expr) => {
        #[must_use = "Future must be awaited to drive the IO operation"]
        pub struct $name {
            fd: Fd,
            io_request: Option<IoRequest>
        }

        impl $name {
            pub fn new(fd: Fd) -> Self {
                Self {
                    fd,
                    io_request: None
                }
            }
        }

        impl Future for $name {
            type Output = std::io::Result<()>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
                let this = unsafe { self.get_unchecked_mut() };
                let worker = unsafe { local_worker() };
                #[allow(unused)]
                let ret;

                poll_for_io_request!((
                     worker.$method(this.fd, this.io_request.as_ref().unwrap_unchecked()),
                     ()
                ));
            }
        }

        #[must_use = "Future must be awaited to drive the IO operation"]
        pub struct $name_with_deadline {
            fd: Fd,
            time_bounded_io_task: TimeBoundedIoTask,
            io_request: Option<IoRequest>
        }

        impl $name_with_deadline {
            pub fn new(fd: Fd, deadline: Instant) -> Self {
                Self {
                    fd,
                    time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
                    io_request: None
                }
            }
        }

        impl Future for $name_with_deadline {
            type Output = std::io::Result<()>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
                let this = unsafe { self.get_unchecked_mut() };
                let worker = unsafe { local_worker() };
                #[allow(unused)]
                let ret;

                poll_for_time_bounded_io_request!((
                     worker.$method(this.fd, this.io_request.as_ref().unwrap_unchecked()),
                     ()
                ));
            }
        }
    }
}

generate_poll!(PollRecv, PollRecvWithDeadline, poll_fd_read);
generate_poll!(PollSend, PollSendWithDeadline, poll_fd_write);

pub trait AsyncPollFd: AsFd {
    #[inline(always)]
    fn poll_recv(&self) -> PollRecv {
        PollRecv::new(self.as_raw_fd())
    }

    #[inline(always)]
    fn poll_recv_with_deadline(&self, deadline: Instant) -> PollRecvWithDeadline {
        PollRecvWithDeadline::new(self.as_raw_fd(), deadline)
    }

    #[inline(always)]
    fn poll_recv_with_timeout(&self, duration: Duration) -> PollRecvWithDeadline {
        self.poll_recv_with_deadline(Instant::now() + duration)
    }

    #[inline(always)]
    fn poll_send(&self) -> PollSend {
        PollSend::new(self.as_raw_fd())
    }

    #[inline(always)]
    fn poll_send_with_deadline(&self, deadline: Instant) -> PollSendWithDeadline {
        PollSendWithDeadline::new(self.as_raw_fd(), deadline)
    }

    #[inline(always)]
    fn poll_send_with_timeout(&self, duration: Duration) -> PollSendWithDeadline {
        self.poll_send_with_deadline(Instant::now() + duration)
    }
}