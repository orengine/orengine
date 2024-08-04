// TODO pub mod simplified_socket;
pub use tcp::{Listener as TcpListener, Stream as TcpStream};
pub use udp::{ConnectedSocket as UdpConnectedSocket, Socket as UdpSocket};
pub use unix::{Listener as UnixListener, Stream as UnixStream};

pub(crate) mod creators_of_sockets;
pub mod tcp;
pub mod udp;
pub mod unix;
