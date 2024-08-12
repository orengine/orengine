use std::{io};
use crate::io::{AsyncBindUnix, AsyncConnectDatagramUnix, AsyncPeekFromUnix, AsyncRecvFromUnix, AsyncSendToUnix};
use crate::net::unix::path_connected_datagram::PathConnectedDatagram;
use crate::net::unix::path_socket::PathSocket;

pub trait PathDatagram<C: PathConnectedDatagram>:
    PathSocket + AsyncBindUnix + AsyncConnectDatagramUnix<C> + AsyncSendToUnix + AsyncRecvFromUnix + AsyncPeekFromUnix {
    async fn pair() -> io::Result<(Self, Self)>;
}