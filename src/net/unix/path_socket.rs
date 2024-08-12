use std::io;
use std::io::Error;

use crate::io::{AsyncClose, AsyncPollFd, AsyncShutdown};
use crate::io::sys::{AsRawFd, AsFd, IntoRawFd, FromRawFd};

pub trait PathSocket: FromRawFd + IntoRawFd + AsRawFd + AsFd + AsyncShutdown + AsyncClose + AsyncPollFd {
    #[inline(always)]
    fn local_addr(&self) -> io::Result<std::os::unix::net::SocketAddr> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.local_addr()?.as_unix().ok_or(Error::new(
            io::ErrorKind::Other,
            "failed to get local address",
        ))
    }

    #[inline(always)]
    fn set_mark(&self, mark: u32) -> io::Result<()> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.set_mark(mark)
    }

    #[inline(always)]
    fn mark(&self) -> io::Result<u32> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.mark()
    }

    #[inline(always)]
    fn take_error(&self) -> io::Result<Option<Error>> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.take_error()
    }
}