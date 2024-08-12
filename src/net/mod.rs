// TODO debug for sockets with local and peer addrs
// TODO bench cpu_affinity on sockets
// TODO add from / to OwnedFd

pub(crate) mod creators_of_sockets;
pub mod tcp;
pub mod udp;
pub mod stream;
pub mod datagram;
pub mod socket;
pub mod connected_datagram;
pub mod listener;

pub use stream::Stream;
pub use datagram::Datagram;
pub use socket::Socket;
pub use connected_datagram::ConnectedDatagram;
pub use listener::Listener;
pub use tcp::{TcpListener, TcpStream};
pub use udp::{UdpConnectedSocket, UdpSocket};