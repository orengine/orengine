use crate::io::sys::{RawFile, RawSocket};

/// `IntoRawSocket` is a synonym for [`IntoRawFd`](std::os::fd::IntoRawFd) on `unix` or for
/// [`IntoRawSocket`](std::os::windows::io::IntoRawSocket) on `windows`.
#[cfg(windows)]
pub trait IntoRawSocket: std::os::windows::io::IntoRawSocket + Sized {
    fn into_raw_socket(self) -> RawSocket {
        std::os::windows::io::IntoRawSocket::into_raw_socket(self)
    }
}

/// `IntoRawSocket` is a synonym for [`IntoRawFd`](std::os::fd::IntoRawFd) on `unix` or for
/// [`IntoRawSocket`](std::os::windows::io::IntoRawSocket) on `windows`.
#[cfg(unix)]
pub trait IntoRawSocket: std::os::fd::IntoRawFd + Sized {
    /// Consumes this object, returning the [`raw underlying socket`](std::os::fd::RawFd).
    ///
    /// This function is typically used to transfer ownership of the underlying raw file
    /// to the caller. When used in this way, callers are then the unique owners of
    /// the raw file and must close it once it's no longer needed.
    #[inline(always)]
    fn into_raw_socket(self) -> RawSocket {
        std::os::fd::IntoRawFd::into_raw_fd(self)
    }
}

/// `IntoRawFile` is a synonym for [`IntoRawFd`](std::os::fd::IntoRawFd) on `unix` or for
/// [`IntoRawHandle`](std::os::windows::io::IntoRawHandle) on `windows`.
#[cfg(windows)]
pub trait IntoRawFile: std::os::windows::io::IntoRawHandle + Sized {
    /// Consumes this object, returning the [`raw underlying file`](std::os::windows::io::RawHandle).
    ///
    /// This function is typically used to transfer ownership of the underlying raw file
    /// to the caller. When used in this way, callers are then the unique owners of
    /// the raw file and must close it once it's no longer needed.
    fn into_raw_file(self) -> RawFile {
        std::os::windows::io::IntoRawHandle::into_raw_handle(self)
    }
}

/// `IntoRawFile` is a synonym for [`IntoRawFd`](std::os::fd::IntoRawFd) on `unix` or for
/// [`IntoRawHandle`](std::os::windows::io::IntoRawHandle) on `windows`.
#[cfg(unix)]
pub trait IntoRawFile: std::os::fd::IntoRawFd + Sized {
    fn into_raw_file(self) -> RawFile {
        std::os::fd::IntoRawFd::into_raw_fd(self)
    }
}

impl IntoRawSocket for std::net::TcpStream {}
impl IntoRawSocket for std::net::TcpListener {}
impl IntoRawSocket for std::net::UdpSocket {}
impl IntoRawSocket for socket2::Socket {}
#[cfg(unix)]
impl IntoRawSocket for std::os::unix::net::UnixStream {}
#[cfg(unix)]
impl IntoRawSocket for std::os::unix::net::UnixDatagram {}
#[cfg(unix)]
impl IntoRawSocket for std::os::unix::net::UnixListener {}
impl IntoRawFile for std::fs::File {}
