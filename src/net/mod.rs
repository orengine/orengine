// TODO pub mod simplified_socket;
pub use tcp::{Listener as TcpListener, Stream as TcpStream};
pub use udp::{ConnectedSocket as UdpConnectedSocket, Socket as UdpSocket};
pub use unix::{Listener as UnixListener, Stream as UnixStream};

// TODO debug for sockets with local and peer addrs
// TODO bench cpu_affinity on sockets

pub(crate) mod creators_of_sockets;
pub mod tcp;
pub mod udp;
pub mod unix;
