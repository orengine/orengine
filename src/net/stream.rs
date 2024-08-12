use std::io;
use std::io::Error;
use std::net::SocketAddr;
use std::time::Duration;
use crate::io::{AsyncConnectStream, AsyncPeek, AsyncRecv, AsyncSend, AsyncShutdown};
use crate::net::Socket;

pub trait Stream: Socket + AsyncConnectStream + AsyncRecv + AsyncPeek + AsyncSend + AsyncShutdown {
    #[inline(always)]
    fn set_linger(&self, linger: Option<Duration>) -> io::Result<()> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.set_linger(linger)
    }

    #[inline(always)]
    fn linger(&self) -> io::Result<Option<Duration>> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.linger()
    }

    #[inline(always)]
    fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.set_nodelay(nodelay)
    }

    #[inline(always)]
    fn nodelay(&self) -> io::Result<bool> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.nodelay()
    }

    #[inline(always)]
    fn peer_addr(&self) -> io::Result<SocketAddr> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.peer_addr()?.as_socket().ok_or(Error::new(
            io::ErrorKind::Other,
            "failed to get local address",
        ))
    }
}