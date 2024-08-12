use std::io;
use std::io::Error;
use crate::io::{AsyncPeek, AsyncRecv, AsyncSend};
use crate::net::unix::path_socket::PathSocket;

pub trait PathConnectedDatagram: PathSocket + AsyncSend + AsyncRecv + AsyncPeek {
    fn peer_addr(&self) -> io::Result<std::os::unix::net::SocketAddr> {
        let borrow_fd = self.as_fd();
        let socket_ref = socket2::SockRef::from(&borrow_fd);
        socket_ref.peer_addr()?.as_unix().ok_or(Error::new(
            io::ErrorKind::Other,
            "failed to get peer address",
        ))
    }
}