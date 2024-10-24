//! This module contains [`TcpListener`].
use std::ffi::c_int;
use std::fmt::{Debug, Formatter};
use std::io::Result;
use std::mem;
use std::net::SocketAddr;

use socket2::{SockAddr, SockRef};

use crate::io::sys::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use crate::io::{AsyncAccept, AsyncBind, AsyncClose};
use crate::net::creators_of_sockets::new_tcp_socket;
use crate::net::tcp::TcpStream;
use crate::net::{BindConfig, Listener};
use crate::runtime::local_executor;

/// A TCP socket server, listening for connections.
///
/// # Close
///
/// [`TcpListener`] is automatically closed after it is dropped.
///
/// # Example
///
/// ```no_run
/// use orengine::io::{AsyncAccept, AsyncBind};
/// use orengine::net::TcpListener;
///
/// # async fn foo() -> std::io::Result<()> {
/// let mut listener = TcpListener::bind("127.0.0.1:8080").await?;
/// while let Ok((stream, addr)) = listener.accept().await {
///     // process the stream
/// }
/// # Ok(())
/// # }
/// ```
pub struct TcpListener {
    pub(crate) fd: RawFd,
}

impl Into<std::net::TcpListener> for TcpListener {
    fn into(self) -> std::net::TcpListener {
        let fd = self.fd;
        mem::forget(self);

        unsafe { std::net::TcpListener::from_raw_fd(fd) }
    }
}

impl From<std::net::TcpListener> for TcpListener {
    fn from(listener: std::net::TcpListener) -> Self {
        Self {
            fd: listener.into_raw_fd(),
        }
    }
}

impl IntoRawFd for TcpListener {
    fn into_raw_fd(self) -> RawFd {
        let fd = self.fd;
        mem::forget(self);

        fd
    }
}

impl AsRawFd for TcpListener {
    #[inline(always)]
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl AsFd for TcpListener {
    fn as_fd(&self) -> BorrowedFd<'_> {
        unsafe { BorrowedFd::borrow_raw(self.fd) }
    }
}

impl From<OwnedFd> for TcpListener {
    fn from(fd: OwnedFd) -> Self {
        unsafe { Self::from_raw_fd(fd.into_raw_fd()) }
    }
}

impl Into<OwnedFd> for TcpListener {
    fn into(self) -> OwnedFd {
        unsafe { OwnedFd::from_raw_fd(self.into_raw_fd()) }
    }
}

impl AsyncBind for TcpListener {
    async fn new_socket(addr: &SocketAddr) -> Result<RawFd> {
        new_tcp_socket(addr).await
    }

    fn bind_and_listen_if_needed(
        sock_ref: SockRef,
        addr: SocketAddr,
        config: &BindConfig,
    ) -> Result<()> {
        sock_ref.bind(&SockAddr::from(addr))?;
        sock_ref.listen(config.backlog_size as c_int)?;

        Ok(())
    }
}

impl AsyncAccept<TcpStream> for TcpListener {}

impl FromRawFd for TcpListener {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self { fd }
    }
}

impl AsyncClose for TcpListener {}

impl Listener for TcpListener {
    type Stream = TcpStream;
}

impl Debug for TcpListener {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut res = f.debug_struct("TcpListener");

        if let Ok(addr) = self.local_addr() {
            res.field("local addr", &addr);
        }

        let name = if cfg!(windows) { "socket" } else { "fd" };
        res.field(name, &self.as_raw_fd()).finish()
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().exec_local_future(async {
            close_future.await.expect("Failed to close tcp listener");
        });
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

    use super::*;
    use crate as orengine;

    #[orengine_macros::test_local]
    fn test_listener() {
        let listener = TcpListener::bind("127.0.0.1:8080")
            .await
            .expect("bind call failed");
        assert_eq!(
            listener.local_addr().unwrap(),
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080))
        );

        listener.set_ttl(122).expect("set_ttl call failed");
        assert_eq!(listener.ttl().expect("ttl call failed"), 122);

        match listener.take_error() {
            Ok(err) => {
                assert!(err.is_none());
            }
            Err(_) => panic!("take_error call failed"),
        }
    }

    // TODO
    // async fn test_listener_accept_with_config(config: &BindConfig) {
    //     let mut listener = TcpListener::bind_with_config("127.0.0.1:4063", config)
    //         .await
    //         .expect("bind call failed");
    //     match listener.accept_with_timeout(Duration::from_micros(1)).await {
    //         Ok(_) => panic!("accept_with_timeout call failed"),
    //         Err(err) => {
    //             assert_eq!(err.kind(), io::ErrorKind::TimedOut);
    //         }
    //     }
    //
    //     let stream = std::net::TcpStream::connect("127.0.0.1:4063").expect("connect call failed");
    //     match listener.accept_with_timeout(Duration::from_secs(1)).await {
    //         Ok((_, addr)) => {
    //             assert_eq!(addr, stream.local_addr().unwrap())
    //         }
    //         Err(_) => panic!("accept_with_timeout call failed"),
    //     }
    //
    //     drop(listener);
    //     yield_now().await;
    // }

    // TODO
    // #[orengine_macros::test_local]
    // fn test_accept() {
    //     let config = BindConfig::default();
    //     test_listener_accept_with_config(&config.reuse_port(ReusePort::Disabled)).await;
    //     test_listener_accept_with_config(&config.reuse_port(ReusePort::Default)).await;
    //     test_listener_accept_with_config(&config.reuse_port(ReusePort::CPU)).await;
    // }
}
