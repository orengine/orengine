use std::io;
use std::io::Error;
use std::net::SocketAddr;
use crate::io::{AsyncPeek, AsyncRecv, AsyncSend};
use crate::net::Socket;

pub trait ConnectedDatagram: Socket + AsyncRecv + AsyncPeek + AsyncSend {
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