use crate::io::{AsyncAccept, AsyncBind};
use crate::net::{Socket, Stream};

/// The `Listener` trait defines common socket-related operations for types that implement
/// listening functionality, such as [`TCP socket listeners`](crate::net::TcpListener).
///
/// It is intended to be implemented for types that accept incoming connections
/// via the [`AsyncAccept`] trait and provides methods for querying and configuring socket settings,
/// such as TTL and obtaining the local address. Additionally, it offers the ability
/// to retrieve pending socket errors.
///
/// # Implemented Traits
///
/// - [`Socket`]
/// - [`AsyncAccept`]
/// - [`AsyncSocketClose`](crate::io::AsyncSocketClose)
/// - [`AsyncBind`]
/// - [`FromRawSocket`](crate::io::sys::FromRawSocket)
/// - [`IntoRawSocket`](crate::io::sys::IntoRawSocket)
/// - [`AsSocket`](crate::io::sys::AsSocket)
/// - [`AsRawSocket`](crate::io::sys::AsRawSocket)
/// - [`AsyncPollSocket`](crate::io::AsyncPollSocket)
pub trait Listener: AsyncAccept<Self::Stream> + Socket + AsyncBind {
    type Stream: Stream<Addr = Self::Addr>;
}
