/// `BorrowedSocket` is a synonym for `BorrowedFd` (`std::os::fd::BorrowedFd`) on `unix`
/// or [`BorrowedSocket`](std::os::windows::io::BorrowedSocket) on `windows`.
#[cfg(windows)]
pub type BorrowedSocket<'socket> = std::os::windows::io::BorrowedSocket<'socket>;

/// `BorrowedSocket` is a synonym for [`BorrowedFd`](std::os::fd::BorrowedFd) on `unix`
/// or `BorrowedSocket` (`std::os::windows::io::BorrowedSocket`) on `windows`.
#[cfg(unix)]
pub type BorrowedSocket<'socket> = std::os::fd::BorrowedFd<'socket>;

/// `BorrowedFile` is a synonym for `BorrowedFd` (`std::os::fd::BorrowedFd`) on `unix`
/// or [`BorrowedHandle`](std::os::windows::io::BorrowedHandle) on `windows`.
#[cfg(windows)]
pub type BorrowedFile<'file> = std::os::windows::io::BorrowedHandle<'file>;

/// `BorrowedFile` is a synonym for [`BorrowedFd`](std::os::fd::BorrowedFd) on `unix`
/// or `BorrowedHandle` (`std::os::windows::io::BorrowedHandle`) on `windows`.
#[cfg(unix)]
pub type BorrowedFile<'file> = std::os::fd::BorrowedFd<'file>;

/// `AsSocket` is as synonyms for `AsFd` (`std::os::fd::AsFd`) on `unix` or for
/// [`AsSocket`](std::os::windows::io::AsSocket) on `windows`.
#[cfg(windows)]
pub trait AsSocket: std::os::windows::io::AsSocket {
    /// Returns a [`BorrowedSocket`] for this socket.
    #[inline]
    fn as_socket(&self) -> BorrowedSocket {
        std::os::windows::io::AsSocket::as_socket(self)
    }
}

/// `AsSocket` is a synonym for [`AsFd`](std::os::fd::AsFd) on `unix` or for
/// `AsSocket` (`std::os::windows::io::AsSocket`) on `windows`.
#[cfg(unix)]
pub trait AsSocket: std::os::fd::AsFd {
    /// Returns a [`BorrowedSocket`] for this socket.
    #[inline]
    fn as_socket(&self) -> BorrowedSocket {
        self.as_fd()
    }
}

/// `AsFile` is a synonym for `AsFd`] (`std::os::fd::AsFd`) on `unix` or for
/// [`AsHandle`](std::os::windows::io::AsHandle) on `windows`.
#[cfg(windows)]
pub trait AsFile: std::os::windows::io::AsHandle {
    /// Returns a [`BorrowedFile`] for this file.
    #[inline]
    fn as_file(&self) -> BorrowedFile {
        self.as_handle()
    }
}

/// `AsFile` is a synonym for [`AsFd`](std::os::fd::AsFd) on `unix` or for
/// `AsHandle` (`std::os::windows::io::AsHandle`) on `windows`.
#[cfg(unix)]
pub trait AsFile: std::os::fd::AsFd {
    /// Returns a [`BorrowedFile`] for this file.
    #[inline]
    fn as_file(&self) -> BorrowedFile {
        self.as_fd()
    }
}

impl AsSocket for std::net::TcpStream {}
impl AsSocket for std::net::TcpListener {}
impl AsSocket for std::net::UdpSocket {}
impl AsSocket for socket2::Socket {}
#[cfg(unix)]
impl AsSocket for std::os::unix::net::UnixStream {}
#[cfg(unix)]
impl AsSocket for std::os::unix::net::UnixDatagram {}
#[cfg(unix)]
impl AsSocket for std::os::unix::net::UnixListener {}
impl AsFile for std::fs::File {}
