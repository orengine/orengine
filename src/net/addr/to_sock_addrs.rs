use crate::net::addr::{FromSockAddr, IntoSockAddr};
use crate::net::unix::UnixAddr;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::Path;

/// `ToSockAddrs` is a copy of [`std::net::ToSocketAddrs`],
/// but with a generic type parameter.
///
/// It allows to use [`UnixAddr`] as a generic type parameter
/// or another custom type that implements [`FromSockAddr`] and [`IntoSockAddr`].
pub trait ToSockAddrs<Addr: FromSockAddr + IntoSockAddr> {
    /// Returned iterator over socket addresses which this type may correspond to.
    type Iter: Iterator<Item = Addr>;

    /// Converts this object to an iterator of resolved `Addr` (generic type of this trait).
    ///
    /// The returned iterator might not actually yield any values depending on the
    /// outcome of any resolution performed.
    ///
    /// Note that this function may block the current thread while resolution is
    /// performed.
    fn to_sock_addrs(&self) -> io::Result<Self::Iter>;
}

impl<T: ToSocketAddrs> ToSockAddrs<SocketAddr> for T {
    type Iter = T::Iter;

    fn to_sock_addrs(&self) -> io::Result<Self::Iter> {
        self.to_socket_addrs()
    }
}

#[cfg(unix)]
impl<T: AsRef<Path>> ToSockAddrs<UnixAddr> for T {
    type Iter = std::iter::Once<UnixAddr>;

    fn to_sock_addrs(&self) -> io::Result<Self::Iter> {
        Ok(std::iter::once(UnixAddr::from(
            std::os::unix::net::SocketAddr::from_pathname(self.as_ref())?,
        )))
    }
}

#[cfg(unix)]
impl ToSockAddrs<Self> for UnixAddr {
    type Iter = std::iter::Once<Self>;

    fn to_sock_addrs(&self) -> io::Result<Self::Iter> {
        Ok(std::iter::once(self.clone()))
    }
}
