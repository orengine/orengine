use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::io::Result;
use std::time::{Duration, Instant};
use io_macros::{poll_for_io_request, poll_for_time_bounded_io_request};
use crate::io::AsyncPollFd;
use crate::io::io_request::{IoRequest};
use crate::io::io_sleeping_task::TimeBoundedIoTask;
use crate::io::sys::{AsFd, Fd};
use crate::io::worker::{IoWorker, local_worker};
use crate::runtime::task::Task;

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct Peek<'a> {
    fd: Fd,
    buf: &'a mut [u8],
    io_request: Option<IoRequest>
}

impl<'a> Peek<'a> {
    pub fn new(fd: Fd, buf: &'a mut [u8]) -> Self {
        Self {
            fd,
            buf,
            io_request: None
        }
    }
}

impl<'a> Future for Peek<'a> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
             worker.peek(this.fd, this.buf.as_mut_ptr(), this.buf.len(), this.io_request.as_ref().unwrap_unchecked()),
             ret
        ));
    }
}

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct PeekWithDeadline<'a> {
    fd: Fd,
    buf: &'a mut [u8],
    time_bounded_io_task: TimeBoundedIoTask,
    io_request: Option<IoRequest>
}

impl<'a> PeekWithDeadline<'a> {
    pub fn new(fd: Fd, buf: &'a mut [u8], deadline: Instant) -> Self {
        Self {
            fd,
            buf,
            time_bounded_io_task: TimeBoundedIoTask::new(deadline, 0),
            io_request: None
        }
    }
}

impl<'a> Future for PeekWithDeadline<'a> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_time_bounded_io_request!((
             worker.peek(this.fd, this.buf.as_mut_ptr(), this.buf.len(), this.io_request.as_ref().unwrap_unchecked()),
             ret
        ));
    }
}

#[macro_export]
macro_rules! generate_peek {
    () => {
         #[inline(always)]
        pub fn peek<'a>(&mut self, buf: &'a mut [u8]) -> crate::io::Peek<'a> {
            crate::io::Peek::new(self.as_raw_fd(), buf)
        }

        #[inline(always)]
        pub fn peek_with_deadline<'a>(&mut self, buf: &'a mut [u8], deadline: Instant) -> crate::io::PeekWithDeadline<'a> {
            crate::io::PeekWithDeadline::new(self.as_raw_fd(), buf, deadline)
        }

        #[inline(always)]
        pub fn peek_with_timeout<'a>(&mut self, buf: &'a mut [u8], duration: Duration) -> crate::io::PeekWithDeadline<'a> {
            self.peek_with_deadline(buf, Instant::now() + duration)
        }
    };
}

#[macro_export]
macro_rules! generate_peek_exact {
    () => {
        #[inline(always)]
        pub async fn peek_exact(&mut self, buf: &mut [u8]) -> Result<()> {
            let mut read = 0;

            while read < buf.len() {
                read += self.peek(&mut buf[read..]).await?;
            }

            Ok(())
        }

        #[inline(always)]
        pub async fn peek_exact_with_deadline(&mut self, buf: &mut [u8], deadline: Instant) -> Result<()> {
            let mut read = 0;

            while read < buf.len() {
                read += self.peek_with_deadline(&mut buf[read..], deadline).await?;
            }

            Ok(())
        }

        #[inline(always)]
        pub async fn peek_exact_with_timeout(&mut self, buf: &mut [u8], duration: Duration) -> Result<()> {
            let mut read = 0;

            while read < buf.len() {
                read += self.peek_with_timeout(&mut buf[read..], duration).await?;
            }

            Ok(())
        }
    };
}