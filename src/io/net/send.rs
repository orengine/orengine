use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use io_macros::{poll_for_io_request, poll_for_time_bounded_io_request};

use crate::io::io_request::IoRequest;
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::Fd;
use crate::io::worker::{local_worker, IoWorker};
use crate::runtime::task::Task;

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct Send<'a> {
    fd: Fd,
    buf: &'a [u8],
    io_request: Option<IoRequest>,
}

impl<'a> Send<'a> {
    pub fn new(fd: Fd, buf: &'a [u8]) -> Self {
        Self {
            fd,
            buf,
            io_request: None,
        }
    }
}

impl<'a> Future for Send<'a> {
    type Output = Result<usize>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
            worker.send(
                this.fd,
                this.buf.as_ptr(),
                this.buf.len(),
                this.io_request.as_ref().unwrap_unchecked()
            ),
            ret
        ));
    }
}

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct SendWithDeadline<'a> {
    fd: Fd,
    buf: &'a [u8],
    time_bounded_io_task: TimeBoundedIoTask,
    io_request: Option<IoRequest>,
}

impl<'a> SendWithDeadline<'a> {
    pub fn new(fd: Fd, buf: &'a [u8], deadline: Instant) -> Self {
        Self {
            fd,
            buf,
            time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
            io_request: None,
        }
    }
}

impl<'a> Future for SendWithDeadline<'a> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_time_bounded_io_request!((
            worker.send(
                this.fd,
                this.buf.as_ptr(),
                this.buf.len(),
                this.io_request.as_ref().unwrap_unchecked()
            ),
            ret
        ));
    }
}

#[macro_export]
macro_rules! generate_send {
    () => {
        #[inline(always)]
        pub fn send<'a>(&mut self, buf: &'a [u8]) -> crate::io::Send<'a> {
            crate::io::Send::new(self.fd, buf)
        }

        #[inline(always)]
        pub fn send_with_deadline<'a>(
            &mut self,
            buf: &'a [u8],
            deadline: Instant,
        ) -> crate::io::SendWithDeadline<'a> {
            crate::io::SendWithDeadline::new(self.fd, buf, deadline)
        }

        #[inline(always)]
        pub fn send_with_timeout<'a>(
            &mut self,
            buf: &'a [u8],
            duration: Duration,
        ) -> crate::io::SendWithDeadline<'a> {
            let deadline = Instant::now() + duration;
            self.send_with_deadline(buf, deadline)
        }
    };
}

#[macro_export]
macro_rules! generate_send_all {
    () => {
        #[inline(always)]
        pub async fn send_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
            let mut written = 0;

            while written < buf.len() {
                written += self.send(&buf[written..]).await?;
            }
            Ok(())
        }

        #[inline(always)]
        pub async fn send_all_with_deadline(
            &mut self,
            buf: &[u8],
            deadline: Instant,
        ) -> std::io::Result<()> {
            let mut written = 0;

            while written < buf.len() {
                written += self.send_with_deadline(&buf[written..], deadline).await?;
            }

            Ok(())
        }

        #[inline(always)]
        pub async fn send_all_with_timeout(
            &mut self,
            buf: &[u8],
            duration: Duration,
        ) -> std::io::Result<()> {
            let mut written = 0;

            while written < buf.len() {
                written += self.send_with_timeout(&buf[written..], duration).await?;
            }

            Ok(())
        }
    };
}
