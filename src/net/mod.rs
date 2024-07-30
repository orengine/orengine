pub mod get_socket;
pub mod tcp;
pub mod udp;

// TODO pub mod simplified_socket;
pub use tcp::{Listener as TcpListener, Stream as TcpStream};
pub use udp::Socket as UdpSocket;
// TODO pub use simplified_socket::SimplifiedSocket;
