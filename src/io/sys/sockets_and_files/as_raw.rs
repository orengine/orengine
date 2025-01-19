/// `RawSocket` is a synonym for `RawFd` (`std::os::fd::RawFd`) on `unix`
/// or [`RawSocket`](std::os::windows::io::RawSocket) on `windows`.
#[cfg(windows)]
pub type RawSocket = std::os::windows::io::RawSocket;

/// `RawSocket` is a synonym for [`RawFd`](std::os::fd::RawFd) on `unix`
/// or `RawSocket` (`std::os::windows::io::RawSocket`) on `windows`.
#[cfg(unix)]
pub type RawSocket = std::os::fd::RawFd;

/// `RawFile` is a synonym for `RawFd` (`std::os::fd::RawFd`) on `unix`
/// or [`RawHandle`](std::os::windows::io::RawHandle) on `windows`.
#[cfg(windows)]
pub type RawFile = std::os::windows::io::RawHandle;

/// `RawFile`is a synonym for [`RawFd`](std::os::fd::RawFd) on `unix`
/// or `RawHandle` (`std::os::windows::io::RawHandle`) on `windows`.
#[cfg(unix)]
pub type RawFile = std::os::fd::RawFd;

/// `AsRawSocket` is a synonym for [`AsRawFd`](std::os::fd::AsRawFd) on `unix` or for
/// `AsRawSocket` (`std::os::windows::io::AsRawSocket`) on `windows`.
#[cfg(unix)]
pub trait AsRawSocket: std::os::fd::AsRawFd {
    /// Returns [`RawSocket`] for this socket.
    #[inline]
    fn as_raw_socket(&self) -> RawSocket {
        std::os::fd::AsRawFd::as_raw_fd(self)
    }
}

/// `AsRawSocket` is a synonym for `AsRawFd` (`std::os::fd::AsRawFd`) on `unix` or for
/// [`AsRawSocket`](std::os::windows::io::AsRawSocket) on `windows`.
#[cfg(windows)]
pub trait AsRawSocket: std::os::windows::io::AsRawSocket {
    /// Returns [`RawSocket`] for this socket.
    #[inline]
    fn as_raw_socket(&self) -> RawSocket {
        std::os::windows::io::AsRawSocket::as_raw_socket(self)
    }
}

/// `AsRawSocket` is a synonym for [`AsRawFd`](std::os::fd::AsRawFd) on `unix` or for
/// `AsRawSocket` (`std::os::windows::io::AsRawSocket`) on `windows`.
#[cfg(unix)]
pub trait AsRawFile: std::os::fd::AsRawFd {
    /// Returns [`RawFile`] for this socket.
    #[inline]
    fn as_raw_file(&self) -> RawFile {
        std::os::fd::AsRawFd::as_raw_fd(self)
    }
}

/// `AsRawFile` is a synonym for `AsRawFd` (`std::os::fd::AsRawFd`) on `unix` or for
/// [`AsRawHandle`](std::os::windows::io::AsRawHandle) on `windows`.
#[cfg(windows)]
pub trait AsRawFile: std::os::windows::io::AsRawHandle {
    /// Returns [`RawFile`] for this file.
    #[inline]
    fn as_raw_file(&self) -> RawFile {
        std::os::windows::io::AsRawHandle::as_raw_handle(self)
    }
}

impl AsRawSocket for std::net::TcpStream {}
impl AsRawSocket for std::net::TcpListener {}
impl AsRawSocket for std::net::UdpSocket {}
impl AsRawSocket for socket2::Socket {}
#[cfg(unix)]
impl AsRawSocket for std::os::unix::net::UnixStream {}
#[cfg(unix)]
impl AsRawSocket for std::os::unix::net::UnixDatagram {}
#[cfg(unix)]
impl AsRawSocket for std::os::unix::net::UnixListener {}
impl AsRawFile for std::fs::File {}
