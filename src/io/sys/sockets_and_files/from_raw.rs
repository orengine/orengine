use crate::io::sys::{RawFile, RawSocket};

/// `FromRawSocket` is a synonym for `FromRawFd` (`std::os::fd::FromRawFd`) on `unix` or for
/// [`FromRawSocket`](std::os::windows::io::FromRawSocket) on `windows`.
#[cfg(windows)]
pub trait FromRawSocket: std::os::windows::io::FromRawSocket + Sized {
    /// Constructs a new instance of `Self` from the given [`RawSocket`].
    ///
    /// # Safety
    ///
    /// The fd passed in must be an [owned socket](std::os::windows::io::OwnedSocket);
    /// in particular, it must be open.
    #[inline]
    unsafe fn from_raw_socket(raw_socket: RawSocket) -> Self {
        <Self as std::os::windows::io::FromRawSocket>::from_raw_socket(raw_socket)
    }
}

/// `FromRawSocket`is a synonym for [`FromRawFd`](std::os::fd::FromRawFd) on `unix` or for
/// `FromRawSocket` (`std::os::windows::io::FromRawSocket`) on `windows`.
#[cfg(unix)]
pub trait FromRawSocket: std::os::fd::FromRawFd + Sized {
    /// Constructs a new instance of `Self` from the given [`RawSocket`].
    ///
    /// # Safety
    ///
    /// The fd passed in must be an [owned socket](std::os::fd::OwnedFd);
    /// in particular, it must be open.
    #[inline]
    unsafe fn from_raw_socket(raw_socket: RawSocket) -> Self {
        std::os::fd::FromRawFd::from_raw_fd(raw_socket)
    }
}

/// `FromRawFile` is a synonym for `FromRawFd` (`std::os::fd::FromRawFd`) on `unix` or for
/// [`FromRawHandle`](std::os::windows::io::FromRawHandle) on `windows`.
#[cfg(windows)]
pub trait FromRawFile: std::os::windows::io::FromRawHandle + Sized {
    /// Constructs a new instance of `Self` from the given [`RawFile`].
    ///
    /// # Safety
    ///
    /// The fd passed in must be an [owned file](std::os::windows::io::OwnedHandle);
    /// in particular, it must be open.
    #[inline]
    unsafe fn from_raw_file(raw_file: RawFile) -> Self {
        std::os::windows::io::FromRawHandle::from_raw_handle(raw_file)
    }
}

/// `FromRawFile` is a synonym for [`FromRawFd`](std::os::fd::FromRawFd) on `unix` or for
/// `FromRawHandle` (`std::os::windows::io::FromRawHandle`) on `windows`.
#[cfg(unix)]
pub trait FromRawFile: std::os::fd::FromRawFd + Sized {
    /// Constructs a new instance of `Self` from the given [`RawFile`].
    ///
    /// # Safety
    ///
    /// The fd passed in must be an [owned file](std::os::fd::OwnedFd);
    /// in particular, it must be open.
    #[inline]
    unsafe fn from_raw_file(raw_file: RawFile) -> Self {
        std::os::fd::FromRawFd::from_raw_fd(raw_file)
    }
}

impl FromRawSocket for std::net::TcpStream {}
impl FromRawSocket for std::net::TcpListener {}
impl FromRawSocket for std::net::UdpSocket {}
impl FromRawSocket for socket2::Socket {}
#[cfg(unix)]
impl FromRawSocket for std::os::unix::net::UnixStream {}
#[cfg(unix)]
impl FromRawSocket for std::os::unix::net::UnixDatagram {}
#[cfg(unix)]
impl FromRawSocket for std::os::unix::net::UnixListener {}
impl FromRawFile for std::fs::File {}
