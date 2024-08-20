use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};
use io_macros::poll_for_io_request;
use crate::io::io_request::{IoRequest};
use crate::io::sys::{AsRawFd, RawFd};
use crate::io::worker::{IoWorker, local_worker};

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct Read<'buf> {
    fd: RawFd,
    buf: &'buf mut [u8],
    io_request: Option<IoRequest>
}

impl<'buf> Read<'buf> {
    pub fn new(fd: RawFd, buf: &'buf mut [u8]) -> Self {
        Self {
            fd,
            buf,
            io_request: None
        }
    }
}

impl<'buf> Future for Read<'buf> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
             worker.read(this.fd, this.buf.as_mut_ptr(), this.buf.len(), this.io_request.as_mut().unwrap_unchecked()),
             ret
        ));
    }
}

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct PositionedRead<'buf> {
    fd: RawFd,
    buf: &'buf mut [u8],
    offset: usize,
    io_request: Option<IoRequest>
}

impl<'buf> PositionedRead<'buf> {
    pub fn new(fd: RawFd, buf: &'buf mut [u8], offset: usize) -> Self {
        Self {
            fd,
            buf,
            offset,
            io_request: None
        }
    }
}

impl<'buf> Future for PositionedRead<'buf> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
             worker.pread(this.fd, this.buf.as_mut_ptr(), this.buf.len(), this.offset, this.io_request.as_mut().unwrap_unchecked()),
             ret
        ));
    }
}

pub trait AsyncRead: AsRawFd {
    #[inline(always)]
    fn read<'buf>(&mut self, buf: &'buf mut [u8]) -> Read<'buf> {
        Read::new(self.as_raw_fd(), buf)
    }

    #[inline(always)]
    fn pread<'buf>(&mut self, buf: &'buf mut [u8], offset: usize) -> PositionedRead<'buf> {
        PositionedRead::new(self.as_raw_fd(), buf, offset)
    }

    #[inline(always)]
    async fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        let mut read = 0;

        while read < buf.len() {
            read += self.read(&mut buf[read..]).await?;
        }

        Ok(())
    }

    #[inline(always)]
    async fn pread_exact(&mut self, buf: &mut [u8], offset: usize) -> Result<()> {
        let mut read = 0;

        while read < buf.len() {
            read += self.pread(&mut buf[read..], offset + read).await?;
        }

        Ok(())
    }
}