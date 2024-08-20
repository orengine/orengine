use std::future::Future;
use std::pin::Pin;
use std::io::Result;
use std::task::{Context, Poll};
use io_macros::{poll_for_io_request};

use crate::io::sys::{AsRawFd, RawFd};
use crate::io::io_request::{IoRequest};
use crate::io::worker::{IoWorker, local_worker};

#[must_use = "Future must be awaited to drive the IO operation"]     
pub struct Write<'buf> {         
    fd: RawFd,
    buf: &'buf [u8],
    io_request: Option<IoRequest>
}     

impl<'buf> Write<'buf> {      
    pub fn new(fd: RawFd, buf: &'buf [u8]) -> Self {
        Self {               
            fd,              
            buf,
            io_request: None
        } 
    }  
}   

impl<'buf> Future for Write<'buf> {  
    type Output = Result<usize>; 
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
             worker.write(this.fd, this.buf.as_ptr(), this.buf.len(), this.io_request.as_mut().unwrap_unchecked()),
             ret
        ));
    }  
}

#[must_use = "Future must be awaited to drive the IO operation"]
pub struct PositionedWrite<'buf> {
    fd: RawFd,
    buf: &'buf [u8],
    offset: usize,
    io_request: Option<IoRequest>
}

impl<'buf> PositionedWrite<'buf> {
    pub fn new(fd: RawFd, buf: &'buf [u8], offset: usize) -> Self {
        Self {
            fd,
            buf,
            offset,
            io_request: None
        }
    }
}

impl<'buf> Future for PositionedWrite<'buf> {
    type Output = Result<usize>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let worker = unsafe { local_worker() };
        let ret;

        poll_for_io_request!((
             worker.pwrite(this.fd, this.buf.as_ptr(), this.buf.len(), this.offset, this.io_request.as_mut().unwrap_unchecked()),
             ret
        ));
    }
}

pub trait AsyncWrite: AsRawFd {
    #[inline(always)]
    fn write<'buf>(&mut self, buf: &'buf [u8]) -> Write<'buf> {
        Write::new(self.as_raw_fd(), buf)
    }

    #[inline(always)]
    fn pwrite<'buf>(&mut self, buf: &'buf [u8], offset: usize) -> PositionedWrite<'buf> {
        PositionedWrite::new(self.as_raw_fd(), buf, offset)
    }

    #[inline(always)]
    async fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        let mut written = 0;

        while written < buf.len() {
            written += self.write(&buf[written..]).await?;
        }
        Ok(())
    }

    #[inline]
    async fn pwrite_all(&mut self, buf: &[u8], offset: usize) -> Result<()> {
        let mut written = 0;
        while written < buf.len() {
            written += self.pwrite(&buf[written..], offset + written).await?;
        }
        Ok(())
    }
}