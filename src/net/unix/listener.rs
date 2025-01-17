//! This module contains [`UnixListener`].
use socket2::{SockAddr, SockRef};
use std::ffi::c_int;
use std::fmt::{Debug, Formatter};
use std::io;
use std::io::ErrorKind::InvalidInput;
use std::io::Result;
use std::mem::ManuallyDrop;

use crate::io::sys::{
    AsRawSocket, AsSocket, BorrowedSocket, FromRawSocket, IntoRawSocket, RawSocket,
};
use crate::io::{AsyncAccept, AsyncBind, AsyncPollSocket, AsyncSocketClose};
use crate::net::addr::ToSockAddrs;
use crate::net::creators_of_sockets::new_unix_stream;
use crate::net::unix::{unix_impl_socket, UnixStream};
use crate::net::{BindConfig, Listener, Socket};
use crate::runtime::local_executor;
use crate::utils::each_addr::each_addr;

/// A structure representing a Unix domain socket server.
///
/// # OS Support
///
/// This structure is only supported on Unix platforms and is not available on Windows.
///
/// # Close
///
/// [`UnixListener`] is automatically closed after it is dropped.
///
/// # Example
///
/// ```rust
/// use orengine::io::{AsyncAccept, AsyncBind};
/// use orengine::net::UnixListener;
///
/// # async fn foo() -> std::io::Result<()> {
/// let mut listener = UnixListener::bind("/tmp/test").await?;
/// while let Ok((stream, addr)) = listener.accept().await {
///     // process the stream
/// }
/// # Ok(())
/// # }
/// ```
pub struct UnixListener {
    pub(crate) raw_socket: RawSocket,
}

impl From<UnixListener> for std::os::unix::net::UnixListener {
    fn from(listener: UnixListener) -> Self {
        unsafe { Self::from_raw_socket(ManuallyDrop::new(listener).raw_socket) }
    }
}

#[allow(
    clippy::fallible_impl_from,
    reason = "std::net::UnixListener is valid."
)]
impl From<std::os::unix::net::UnixListener> for UnixListener {
    fn from(listener: std::os::unix::net::UnixListener) -> Self {
        Self {
            raw_socket: IntoRawSocket::into_raw_socket(listener),
        }
    }
}

impl std::os::fd::IntoRawFd for UnixListener {
    fn into_raw_fd(self) -> std::os::fd::RawFd {
        ManuallyDrop::new(self).raw_socket
    }
}

impl IntoRawSocket for UnixListener {}

impl std::os::fd::AsRawFd for UnixListener {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.raw_socket
    }
}

impl AsRawSocket for UnixListener {}

impl std::os::fd::AsFd for UnixListener {
    fn as_fd(&self) -> std::os::fd::BorrowedFd {
        unsafe { std::os::fd::BorrowedFd::borrow_raw(self.raw_socket) }
    }
}

impl AsSocket for UnixListener {}

impl std::os::fd::FromRawFd for UnixListener {
    unsafe fn from_raw_fd(raw_fd: std::os::fd::RawFd) -> Self {
        Self { raw_socket: raw_fd }
    }
}

impl FromRawSocket for UnixListener {}

impl AsyncPollSocket for UnixListener {}

impl Socket for UnixListener {
    unix_impl_socket!();
}

impl AsyncBind for UnixListener {
    async fn new_socket(_: &Self::Addr) -> Result<RawSocket> {
        new_unix_stream().await
    }

    fn bind_and_listen_if_needed(
        sock_ref: SockRef,
        addr: Self::Addr,
        config: &BindConfig,
    ) -> Result<()> {
        sock_ref
            .bind(&SockAddr::unix(addr.as_pathname().ok_or_else(|| {
                io::Error::new(InvalidInput, "Invalid address.")
            })?)?)?;
        #[allow(clippy::cast_possible_truncation, reason = "we have to cast it")]
        sock_ref.listen(config.backlog_size as c_int)?;

        Ok(())
    }

    async fn bind_with_config<A: ToSockAddrs<Self::Addr>>(
        addrs: A,
        config: &BindConfig,
    ) -> Result<Self> {
        each_addr(addrs, |addr| async move {
            let raw_socket = Self::new_socket(&addr).await?;
            let borrow_socket = unsafe { BorrowedSocket::borrow_raw(raw_socket) };
            let sock_ref = socket2::SockRef::from(&borrow_socket);

            Self::bind_and_listen_if_needed(sock_ref, addr, config)
                .map(|_| UnixListener { raw_socket })
        })
        .await
    }
}

impl AsyncAccept<UnixStream> for UnixListener {}

impl AsyncSocketClose for UnixListener {}

impl Listener for UnixListener {
    type Stream = UnixStream;
}

impl Debug for UnixListener {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut res = f.debug_struct("UnixListener");

        if let Ok(addr) = self.local_addr() {
            res.field("local addr", &addr);
        }

        res.field("raw_socket", &AsRawSocket::as_raw_socket(self))
            .finish()
    }
}

impl Drop for UnixListener {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().exec_local_future(async {
            close_future.await.expect("Failed to close unix listener");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::fs;
    use std::time::Duration;

    #[orengine::test::test_local]
    fn test_listener() {
        const ADDR: &str = "/tmp/orengine_test_listener";

        let _ = fs::remove_file(ADDR).await;

        let listener = UnixListener::bind(ADDR).await.expect("bind call failed");
        assert_eq!(
            listener.local_addr().unwrap().as_pathname().unwrap(),
            std::os::unix::net::SocketAddr::from_pathname(ADDR)
                .unwrap()
                .as_pathname()
                .unwrap()
        );
        assert_eq!(
            listener
                .local_addr()
                .unwrap()
                .as_pathname()
                .unwrap()
                .to_string_lossy(),
            ADDR
        );

        assert!(listener
            .take_error()
            .expect("take_error call failed")
            .is_none());
    }

    #[orengine::test::test_local]
    fn test_accept() {
        let addr = "/tmp/orengine_test_listener_accept";
        let _ = fs::remove_file(addr).await;

        let mut listener = UnixListener::bind(addr).await.expect("bind call failed");
        match listener.accept_with_timeout(Duration::from_micros(1)).await {
            Ok(_) => panic!("accept_with_timeout call should failed"),
            Err(err) => {
                assert_eq!(err.kind(), io::ErrorKind::TimedOut);
            }
        }

        let stream = std::os::unix::net::UnixStream::connect(addr).expect("connect call failed");

        listener
            .accept_with_timeout(Duration::from_secs(1))
            .await
            .expect("accept_with_timeout call failed");

        drop(stream);
    }
}
