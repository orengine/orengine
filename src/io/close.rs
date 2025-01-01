use crate as orengine;
use crate::io::io_request_data::IoRequestData;
use crate::io::worker::{local_worker, IoWorker};

use crate::io::sys::{AsRawFile, AsRawSocket, RawFile, RawSocket};
use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

/// `close` io operation for sockets.
pub struct CloseSocket {
    raw_socket: RawSocket,
    io_request_data: Option<IoRequestData>,
}

impl CloseSocket {
    /// Create a new `CloseSocket` future.
    pub fn new(raw_socket: RawSocket) -> Self {
        Self {
            raw_socket,
            io_request_data: None,
        }
    }
}

impl Future for CloseSocket {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        #[allow(unused, reason = "Cannot write proc_macro else to make it readable.")]
        let ret;

        poll_for_io_request!((
            local_worker().close_socket(this.raw_socket, unsafe {
                this.io_request_data.as_mut().unwrap_unchecked()
            }),
            ()
        ));
    }
}

unsafe impl Send for CloseSocket {}

/// The [`AsyncSocketClose`] trait represents an asynchronous close operation for sockets.
///
/// This trait can be implemented for all types that implement [`AsRawSocket`].
pub trait AsyncSocketClose: AsRawSocket {
    /// Returns future that closes a provided socket.
    ///
    /// # Be careful
    ///
    /// Some structs (like all structs in [`orengine::net`](crate::net)
    /// implements [`Drop`](Drop) that calls [`close`](Self::close).
    ///
    /// So, before call [`close`](Self::close) you should check if the struct implements auto-closing.
    fn close(&mut self) -> impl Future<Output = Result<()>> {
        CloseSocket::new(self.as_raw_fd())
    }
}

/// `close` io operation for files.
pub struct CloseFile {
    raw_file: RawFile,
    io_request_data: Option<IoRequestData>,
}

impl CloseFile {
    /// Create a new `CloseFile` future.
    pub fn new(raw_file: RawFile) -> Self {
        Self {
            raw_file,
            io_request_data: None,
        }
    }
}

impl Future for CloseFile {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        #[allow(unused, reason = "Cannot write proc_macro else to make it readable.")]
        let ret;

        poll_for_io_request!((
            local_worker().close_file(this.raw_file, unsafe {
                this.io_request_data.as_mut().unwrap_unchecked()
            }),
            ()
        ));
    }
}

unsafe impl Send for CloseFile {}

/// The [`AsyncSocketClose`] trait represents an asynchronous close operation for files.
///
/// This trait can be implemented for all types that implement [`AsRawFile`].
pub trait AsyncFileClose: AsRawFile {
    /// Returns future that closes a provided file.
    ///
    /// # Be careful
    ///
    /// Some structs (like all structs in [`orengine::fs`](crate::fs)) implements [`Drop`](Drop)
    /// that calls [`close`](Self::close).
    ///
    /// So, before call [`close`](Self::close) you should check if the struct implements auto-closing.
    fn close(&mut self) -> impl Future<Output = Result<()>> {
        CloseFile::new(self.as_raw_fd())
    }
}
