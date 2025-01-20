use crate as orengine;
use crate::io::io_request_data::{IoRequestData, IoRequestDataPtr};
use crate::io::sys::{AsRawSocket, RawSocket};
use crate::io::worker::{local_worker, IoWorker};
use crate::net::Socket;
use orengine_macros::poll_for_io_request;
use std::future::Future;
use std::io::Result;
use std::net::Shutdown as ShutdownHow;
use std::pin::Pin;
use std::task::{Context, Poll};

/// `shutdown` io operation.
#[repr(C)]
pub struct Shutdown {
    raw_socket: RawSocket,
    how: ShutdownHow,
    io_request_data: Option<IoRequestData>,
}

impl Shutdown {
    /// Creates new `shutdown` io operation.
    pub fn new(raw_socket: RawSocket, how: ShutdownHow) -> Self {
        Self {
            raw_socket,
            how,
            io_request_data: None,
        }
    }
}

impl Future for Shutdown {
    type Output = Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = &mut *self;
        #[allow(unused, reason = "Cannot write proc_macro else to make it readable.")]
        let ret;

        poll_for_io_request!((
            local_worker().shutdown(this.raw_socket, this.how, unsafe {
                IoRequestDataPtr::new(this.io_request_data.as_mut().unwrap_unchecked())
            }),
            ()
        ));
    }
}

unsafe impl Send for Shutdown {}

/// The `AsyncShutdown` trait provides a method for asynchronously shutting down part or all of a
/// connection.
///
/// It can be implemented for any [`sockets`](Socket).
/// The trait leverages different shutdown options [`Shutdown`](std::net::Shutdown)
/// to control which aspects of the connection to shut down, such as reading, writing, or both.
///
/// # Example
///
/// ```rust
/// use std::net::Shutdown;
/// use orengine::net::TcpStream;
/// use orengine::io::{AsyncConnectStream, AsyncShutdown};
///
/// # async fn foo() -> std::io::Result<()> {
/// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
///
/// // Shutdown the writing half of the connection
/// stream.shutdown(Shutdown::Write).await?;
/// # Ok(())
/// # }
/// ```
pub trait AsyncShutdown: Socket {
    /// Shuts down part or all of the connection. The shutdown behavior is determined by the
    /// [`Shutdown`](std::net::Shutdown) enum, which specifies whether to shut down reading,
    /// writing, or both.
    ///
    /// This method immediately issues the shutdown command. However, any data already queued for
    /// transmission or reception may still be processed.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::net::Shutdown;
    /// use orengine::net::TcpStream;
    /// use orengine::io::{AsyncConnectStream, AsyncShutdown};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let mut stream = TcpStream::connect("127.0.0.1:8080").await?;
    ///
    /// // Shut down both reading and writing
    /// stream.shutdown(Shutdown::Both).await?;
    /// # Ok(())
    /// # }
    /// ```
    fn shutdown(&mut self, how: ShutdownHow) -> impl Future<Output = Result<()>> {
        Shutdown::new(AsRawSocket::as_raw_socket(self), how)
    }
}
