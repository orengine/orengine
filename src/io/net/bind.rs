use crate::each_addr;
use crate::io::sys::{BorrowedSocket, FromRawSocket, RawSocket};
use crate::net::{BindConfig, ReusePort};
use socket2::SockRef;
use std::io::Result;
use std::net::{SocketAddr, ToSocketAddrs};

/// The `AsyncBind` trait provides asynchronous methods for creating, binding, and configuring
/// sockets.
///
/// It is primarily used to bind a socket to a specific address, with options for setting
/// configuration such as address reuse and port reuse.
///
/// This trait can be implemented for types that represent sockets, and it provides both direct binding
/// and binding with custom configuration options via [`BindConfig`].
///
/// # Example
///
/// ```rust
/// use orengine::net::TcpListener;
/// use orengine::net::BindConfig;
/// use orengine::io::AsyncBind;
///
/// # async fn foo() -> std::io::Result<()> {
/// // Create a listener bound to the local address
/// let listener = TcpListener::bind("127.0.0.1:8080").await?;
///
/// // Or bind with a custom configuration
/// let config = BindConfig {
///     reuse_address: true,
///     reuse_port: orengine::net::ReusePort::Default,
///     backlog_size: 1024,
///     only_v6: false
/// };
/// let listener = TcpListener::bind_with_config("127.0.0.1:8080", &config).await?;
///
/// # Ok(())
/// # }
/// ```
pub trait AsyncBind: Sized + FromRawSocket {
    /// Creates a new socket that can be bound to the specified address.
    ///
    /// This method is responsible for creating a raw file descriptor (socket) and returning the
    /// associated file descriptor.
    ///
    /// The address here is only used to set the correct domain.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::net::SocketAddr;
    /// use orengine::net::TcpListener;
    /// use orengine::io::AsyncBind;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    /// let raw_fd = TcpListener::new_socket(&addr).await?;
    ///
    /// // set the raw_fd up here and bind it
    /// # Ok(())
    /// # }
    /// ```
    async fn new_socket(addr: &SocketAddr) -> Result<RawSocket>;

    /// Binds the socket and listens on the provided address if needed, applying the provided
    /// configuration from [`BindConfig`].
    ///
    /// - TCP listener binds to an address and port and listens for incoming connections.
    ///
    /// - UDP socket only binds to an address.
    ///
    /// This method is used internally to set options like address reuse, port reuse, and IPv6 handling.
    ///
    /// # Parameters
    /// * `sock_ref` - A reference to the socket file descriptor.
    /// * `addr` - The socket address to bind to.
    /// * `config` - The configuration for binding, defined by [`BindConfig`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::{BindConfig, TcpListener};
    /// use orengine::socket2::SockRef;
    /// use orengine::io::{AsyncBind, sys::BorrowedSocket};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let config = BindConfig::default();
    /// let addr = "127.0.0.1:8080".parse().unwrap();
    /// let raw_socket = TcpListener::new_socket(&addr).await?;
    /// let borrowed_raw_fd = unsafe { BorrowedSocket::borrow_raw(raw_socket) };
    /// let sock_ref = SockRef::from(&borrowed_raw_fd);
    ///
    /// TcpListener::bind_and_listen_if_needed(sock_ref, addr, &config)?;
    /// # Ok(())
    /// # }
    /// ```
    fn bind_and_listen_if_needed(
        sock_ref: SockRef,
        addr: SocketAddr,
        config: &BindConfig,
    ) -> Result<()>;

    /// Asynchronously binds to a socket with a specific [`configuration`](BindConfig).
    ///
    /// It will bind to first valid address provided in the list of possible addresses.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpListener;
    /// use orengine::net::BindConfig;
    /// use orengine::io::AsyncBind;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let config = BindConfig::default().only_v6(true);
    /// let listener = TcpListener::bind_with_config("127.0.0.1:8080", &config).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn bind_with_config<A: ToSocketAddrs>(addrs: A, config: &BindConfig) -> Result<Self> {
        each_addr!(&addrs, move |addr| async move {
            let raw_fd = Self::new_socket(&addr).await?;
            let borrowed_raw_fd = unsafe { BorrowedSocket::borrow_raw(raw_fd) };
            let socket_ref = socket2::SockRef::from(&borrowed_raw_fd);

            if config.only_v6 {
                socket_ref.set_only_v6(true)?;
            }

            if config.reuse_address {
                socket_ref.set_reuse_address(true)?;
            }

            match config.reuse_port {
                ReusePort::Disabled => {
                    Self::bind_and_listen_if_needed(socket_ref, addr, config)?;
                }
                ReusePort::Default => {
                    #[cfg(unix)]
                    {
                        socket_ref.set_reuse_port(true)?;

                        Self::bind_and_listen_if_needed(socket_ref, addr, config)?;
                    }

                    #[cfg(windows)]
                    {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "CPU reuse is not supported on windows",
                        ));
                    }
                }
                ReusePort::CPU => {
                    #[cfg(unix)]
                    {
                        socket_ref.set_reuse_port(true)?;
                    }

                    #[cfg(windows)]
                    {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "CPU reuse is not supported on windows",
                        ));
                    }

                    #[cfg(target_os = "linux")]
                    {
                        Self::bind_and_listen_if_needed(socket_ref, addr, config)?;

                        use libc::{
                            self, __u32, BPF_ABS, BPF_LD, BPF_RET, BPF_W, SKF_AD_CPU, SKF_AD_OFF,
                        };
                        const BPF_A: __u32 = 0x10;

                        // [
                        //     {BPF_LD | BPF_W | BPF_ABS, 0, 0, SKF_AD_OFF + SKF_AD_CPU},
                        //     {BPF_RET | BPF_A, 0, 0, 0}
                        // ]
                        #[allow(clippy::cast_possible_truncation, reason = "They are flags")]
                        #[allow(clippy::cast_sign_loss, reason = "They are flags")]
                        let mut code = [
                            libc::sock_filter {
                                code: (BPF_LD | BPF_W | BPF_ABS) as _,
                                jt: 0,
                                jf: 0,
                                k: (SKF_AD_OFF + SKF_AD_CPU) as _,
                            },
                            libc::sock_filter {
                                code: (BPF_RET | BPF_A) as _,
                                jt: 0,
                                jf: 0,
                                k: 0,
                            },
                        ];
                        let p = libc::sock_fprog {
                            len: 2,
                            filter: code.as_mut_ptr(),
                        };
                        #[allow(
                            clippy::cast_possible_truncation,
                            reason = "size of libc::sock_fprog is less than u32::MAX"
                        )]
                        let res = unsafe {
                            libc::setsockopt(
                                raw_fd as _,
                                libc::SOL_SOCKET,
                                libc::SO_ATTACH_REUSEPORT_CBPF,
                                std::ptr::from_ref(&p).cast(),
                                size_of::<libc::sock_fprog>() as _,
                            )
                        };
                        if res < 0 {
                            return Err(std::io::Error::last_os_error());
                        }
                    }
                }
            }

            Ok(unsafe { <Self as FromRawSocket>::from_raw_socket(raw_fd) })
        })
    }

    /// Asynchronously binds to a socket with default [`configuration`](BindConfig).
    ///
    /// This is a convenience method that uses [`BindConfig::default`] to bind to the specified
    /// address without any special options.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::net::TcpListener;
    /// use orengine::io::AsyncBind;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let listener = TcpListener::bind("127.0.0.1:8080").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    async fn bind<A: ToSocketAddrs>(addrs: A) -> Result<Self> {
        Self::bind_with_config(addrs, &BindConfig::default()).await
    }
}
