use libc::{sa_family_t, sockaddr_storage, socklen_t};
use socket2::SockAddr;
use std::ffi::OsStr;
use std::mem::{offset_of, MaybeUninit};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::SocketAddr;
use std::path::Path;
use std::{fmt, io, mem, ptr};

/// An offset to the `sun_path` field of `sockaddr_un`.
const SUN_PATH_OFFSET: usize = offset_of!(libc::sockaddr_un, sun_path);

pub(in crate::net) fn sockaddr_un(path: &Path) -> io::Result<(sockaddr_storage, socklen_t)> {
    // SAFETY: All zeros is a valid representation for `sockaddr_un`.
    let mut storage = unsafe { mem::zeroed::<sockaddr_storage>() };
    let unix_addr_ref = unsafe { &mut *(&raw mut storage).cast::<libc::sockaddr_un>() };
    #[allow(clippy::cast_possible_truncation, reason = "libc::AF_UNIX is 1")]
    {
        unix_addr_ref.sun_family = libc::AF_UNIX as sa_family_t;
    }

    let bytes = path.as_os_str().as_bytes();

    if memchr::memchr(0, bytes).is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "paths must not contain null bytes",
        ));
    }

    if bytes.len() >= unix_addr_ref.sun_path.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must be shorter than SUN_LEN",
        ));
    }
    // SAFETY: `bytes` and `addr.sun_path` are not overlapping and
    // both point to valid memory.
    // NOTE: We zeroed the memory above, so the path is already null
    // terminated.
    unsafe {
        ptr::copy_nonoverlapping(
            bytes.as_ptr(),
            unix_addr_ref.sun_path.as_mut_ptr().cast(),
            bytes.len(),
        );
    };

    let mut len = SUN_PATH_OFFSET + bytes.len();
    match bytes.first() {
        Some(&0) | None => {}
        Some(_) => len += 1,
    }
    #[allow(clippy::cast_possible_truncation, reason = "len is less than u32::MAX")]
    Ok((storage, len as socklen_t))
}

#[derive(PartialEq, Eq, Debug)]
enum AddressKind<'a> {
    Unnamed,
    Pathname(&'a Path),
    Abstract(&'a [u8]),
}

/// `UnixAddr` is a wrapper around `socket2::SockAddr`.
/// It is more readable and implements [`ToSockAddrs`], [`IntoSockAddr`] and [`FromSockAddr`].
///
/// [`ToSockAddrs`]: crate::net::addr::ToSockAddrs
/// [`IntoSockAddr`]: crate::net::addr::IntoSockAddr
/// [`FromSockAddr`]: crate::net::addr::FromSockAddr
#[derive(Clone)]
pub struct UnixAddr {
    inner: SockAddr,
}

impl UnixAddr {
    /// Constructs a `SockAddr` with the family `AF_UNIX` and the provided path.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is longer than `SUN_LEN` or if it contains
    /// NULL bytes.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use orengine::net::unix::UnixAddr;
    /// use std::path::Path;
    ///
    /// # fn main() -> std::io::Result<()> {
    /// let address = UnixAddr::from_pathname("/path/to/socket")?;
    /// assert_eq!(address.as_pathname(), Some(Path::new("/path/to/socket")));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Creating a `UnixAddr` with a NULL byte results in an error.
    ///
    /// ```rust
    /// use orengine::net::unix::UnixAddr;
    ///
    /// assert!(UnixAddr::from_pathname("/path/with/\0/bytes").is_err());
    /// ```
    // Forked from `std::os::unix::net::SocketAddr::from_pathname`
    pub fn from_pathname<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        sockaddr_un(path.as_ref()).map(|(addr, len)| Self {
            inner: unsafe { SockAddr::new(addr, len) },
        })
    }

    // Forked from `std::os::unix::net::SocketAddr::address`
    fn address(&self) -> AddressKind<'_> {
        let unix_addr_ref =
            unsafe { &mut *self.inner.as_ptr().cast_mut().cast::<libc::sockaddr_un>() };
        let len = self.inner.len() as usize - SUN_PATH_OFFSET;
        let path = unsafe { &*((&raw const unix_addr_ref.sun_path).cast::<[u8; 108]>()) };

        // macOS seems to return a len of 16 and a zeroed sun_path for unnamed addresses
        if len == 0
            || (cfg!(not(any(target_os = "linux", target_os = "android")))
                && unix_addr_ref.sun_path[0] == 0)
        {
            AddressKind::Unnamed
        } else if unix_addr_ref.sun_path[0] == 0 {
            AddressKind::Abstract(&path[1..len])
        } else {
            AddressKind::Pathname(OsStr::from_bytes(&path[..len - 1]).as_ref())
        }
    }

    /// Returns the contents of this address if it is a `pathname` address.
    ///
    /// # Examples
    ///
    /// With a pathname:
    ///
    /// ```no_run
    /// use orengine::net::{UnixListener, Socket};
    /// use orengine::io::AsyncBind;
    /// use std::path::Path;
    ///
    /// async fn foo() -> std::io::Result<()> {
    ///     let socket = UnixListener::bind("/tmp/sock").await?;
    ///     let addr = socket.local_addr().expect("Couldn't get local address");
    ///     assert_eq!(addr.as_pathname(), Some(Path::new("/tmp/sock")));
    ///     Ok(())
    /// }
    /// ```
    ///
    /// Without a pathname:
    ///
    /// ```no_run
    /// use orengine::net::{UnixDatagram, Socket};
    /// use orengine::io::AsyncBind;
    /// use orengine::io::sys::FromRawSocket;
    /// use orengine::net::unix::UnixAddr;
    ///
    /// async fn foo() -> std::io::Result<()> {
    ///     let empty_addr: UnixAddr = unsafe { std::mem::zeroed() }; // addr is needed only for set sock_addr type, but in unix it is ignored
    ///     let raw_socket = UnixDatagram::new_socket(&empty_addr).await?; // Create an unbound socket
    ///     let socket = unsafe { UnixDatagram::from_raw_socket(raw_socket) };
    ///     let addr = socket.local_addr().expect("Couldn't get local address");
    ///     assert_eq!(addr.as_pathname(), None);
    ///     Ok(())
    /// }
    /// ```
    pub fn as_pathname(&self) -> Option<&Path> {
        if let AddressKind::Pathname(path) = self.address() {
            Some(path)
        } else {
            None
        }
    }

    /// Returns whether this address is unnamed.
    ///
    /// # Examples
    ///
    /// A named address:
    ///
    /// ```no_run
    /// use orengine::net::{UnixListener, Socket};
    /// use orengine::io::AsyncBind;
    ///
    /// async fn foo() -> std::io::Result<()> {
    ///     let socket = UnixListener::bind("/tmp/sock").await?;
    ///     let addr = socket.local_addr().expect("Couldn't get local address");
    ///     assert_eq!(addr.is_unnamed(), false);
    ///     Ok(())
    /// }
    /// ```
    ///
    /// An unnamed address:
    ///
    /// ```
    /// use orengine::net::{UnixDatagram, Socket};
    /// use orengine::io::AsyncBind;
    /// use orengine::net::unix::UnixAddr;
    /// use orengine::io::sys::FromRawSocket;
    ///
    /// async fn foo() -> std::io::Result<()> {
    ///     let empty_addr: UnixAddr = unsafe { std::mem::zeroed() }; // addr is needed only for set sock_addr type, but in unix it is ignored
    ///     let raw_socket = UnixDatagram::new_socket(&empty_addr).await?; // Create an unbound socket
    ///     let socket = unsafe { UnixDatagram::from_raw_socket(raw_socket) };
    ///     let addr = socket.local_addr().expect("Couldn't get local address");
    ///     assert_eq!(addr.is_unnamed(), true);
    ///     Ok(())
    /// }
    /// ```
    #[must_use]
    pub fn is_unnamed(&self) -> bool {
        matches!(self.address(), AddressKind::Unnamed)
    }
}

#[cfg(any(doc, target_os = "android", target_os = "linux"))]
#[cfg(any(target_os = "android", target_os = "linux"))]
impl UnixAddr {
    /// Returns the contents of this address if it is in the abstract namespace.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use orengine::net::unix::{UnixListener, UnixAddr};
    /// use orengine::io::AsyncBind;
    /// use orengine::net::Socket;
    ///
    /// async fn foo() -> std::io::Result<()> {
    ///     let name = b"hidden";
    ///     let name_addr = UnixAddr::from_abstract_name(name)?;
    ///     let socket = UnixListener::bind(name_addr).await?;
    ///     let local_addr = socket.local_addr().expect("Couldn't get local address");
    ///     assert_eq!(local_addr.as_abstract_name(), Some(&name[..]));
    ///     Ok(())
    /// }
    /// ```
    // Forked from `std::os::unix::net::SocketAddr::as_abstract_name`
    pub fn as_abstract_name(&self) -> Option<&[u8]> {
        if let AddressKind::Abstract(name) = self.address() {
            Some(name)
        } else {
            None
        }
    }

    /// Creates a Unix socket address in the abstract namespace.
    ///
    /// The abstract namespace is a Linux-specific extension that allows Unix
    /// sockets to be bound without creating an entry in the filesystem.
    /// Abstract sockets are unaffected by filesystem layout or permissions,
    /// and no cleanup is necessary when the socket is closed.
    ///
    /// An abstract socket address name may contain any bytes, including zero.
    ///
    /// # Errors
    ///
    /// Returns an error if the name is longer than `SUN_LEN - 1`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use orengine::net::unix::{UnixListener, UnixAddr};
    /// use orengine::io::AsyncBind;
    ///
    /// async fn foo() -> std::io::Result<()> {
    ///     let addr = UnixAddr::from_abstract_name(b"hidden")?;
    ///     let listener = match UnixListener::bind(addr).await {
    ///         Ok(sock) => sock,
    ///         Err(err) => {
    ///             println!("Couldn't bind: {err:?}");
    ///             return Err(err);
    ///         }
    ///     };
    ///     Ok(())
    /// }
    /// ```
    // Forked from `std::os::unix::net::SocketAddr::from_abstract_name`
    pub fn from_abstract_name<N>(name: N) -> io::Result<Self>
    where
        N: AsRef<[u8]>,
    {
        let name = name.as_ref();
        unsafe {
            // SAFETY: All zeros is a valid representation for `sockaddr_un`.
            let mut storage = mem::zeroed::<sockaddr_storage>();
            let unix_addr_ref = &mut *(&raw mut storage).cast::<libc::sockaddr_un>();
            #[allow(clippy::cast_possible_truncation, reason = "libc::AF_UNIX is 1")]
            {
                unix_addr_ref.sun_family = libc::AF_UNIX as sa_family_t;
            }

            if name.len() + 1 > unix_addr_ref.sun_path.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "abstract socket name must be shorter than SUN_LEN",
                ));
            }

            ptr::copy_nonoverlapping(
                name.as_ptr(),
                unix_addr_ref.sun_path.as_mut_ptr().add(1).cast(),
                name.len(),
            );

            #[allow(
                clippy::cast_possible_truncation,
                reason = "SUN_PATH_OFFSET + 1 + name.len() < u32::MAX"
            )]
            let len = (SUN_PATH_OFFSET + 1 + name.len()) as socklen_t;

            Ok(Self {
                inner: SockAddr::new(storage, len),
            })
        }
    }
}

/// We use a pointer arithmetic because we know that [`SocketAddr`] is:
///
/// ```
/// pub struct SocketAddr {
///     addr: libc::sockaddr_un,
///     len: libc::socklen_t,
/// }
/// ```
///
/// It is a copy that is used to get offsets of fields.
pub struct SocketAddrPrototype {
    pub(in crate::net) addr: libc::sockaddr_un,
    pub(in crate::net) len: socklen_t,
}

impl From<SocketAddr> for UnixAddr {
    fn from(socket_addr: SocketAddr) -> Self {
        let mut addr: SockAddr = unsafe { mem::zeroed() }; // SocketAddr < SockAddr, so we need to fill it with zeros
        let len = unsafe {
            ptr::read_unaligned(
                (&raw const socket_addr)
                    .byte_add(offset_of!(SocketAddrPrototype, len))
                    .cast::<socklen_t>(),
            )
        };

        unsafe {
            ptr::copy_nonoverlapping::<libc::sockaddr_un>(
                (&raw const socket_addr)
                    .byte_add(offset_of!(SocketAddrPrototype, addr))
                    .cast(),
                addr.as_ptr().cast::<libc::sockaddr_un>().cast_mut(),
                1,
            );
            addr.set_length(len);
        }

        Self { inner: addr }
    }
}

impl From<UnixAddr> for SocketAddr {
    fn from(unix_addr: UnixAddr) -> Self {
        let mut addr = MaybeUninit::<Self>::uninit(); // we rewrite it all
        let addr_ptr = addr.as_mut_ptr();

        unsafe {
            ptr::copy_nonoverlapping::<libc::sockaddr_un>(
                unix_addr.inner.as_ptr().cast(),
                addr_ptr
                    .byte_add(offset_of!(SocketAddrPrototype, addr))
                    .cast::<libc::sockaddr_un>(),
                1,
            );

            let len = unix_addr.inner.len() as socklen_t;
            let addr_len_ptr = addr_ptr
                .byte_add(offset_of!(SocketAddrPrototype, len))
                .cast::<socklen_t>();
            ptr::write(addr_len_ptr, len);

            addr.assume_init()
        }
    }
}

impl From<SockAddr> for UnixAddr {
    fn from(addr: SockAddr) -> Self {
        Self { inner: addr }
    }
}

impl From<UnixAddr> for SockAddr {
    fn from(addr: UnixAddr) -> Self {
        addr.inner
    }
}

impl AsRef<SockAddr> for UnixAddr {
    fn as_ref(&self) -> &SockAddr {
        &self.inner
    }
}

impl fmt::Debug for UnixAddr {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.address() {
            AddressKind::Unnamed => write!(fmt, "(unnamed)"),
            AddressKind::Abstract(name) => write!(fmt, "\"{}\" (abstract)", name.escape_ascii()),
            AddressKind::Pathname(path) => write!(fmt, "{path:?} (pathname)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::net::unix::addr::AddressKind;
    use crate::net::unix::UnixAddr;
    #[cfg(any(target_os = "android", target_os = "linux"))]
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::SocketAddr;

    #[test]
    fn test_unix_addr_from() {
        match UnixAddr::from_pathname("/tmp/sock") {
            Ok(addr) => assert_eq!(addr.as_pathname(), Some("/tmp/sock".as_ref())),
            Err(err) => panic!("Unexpected error: {err}"),
        }

        assert!(
            UnixAddr::from_pathname("/tmp/s\0ock").is_err(),
            "Path with NULL byte must be rejected"
        );

        #[cfg(any(target_os = "android", target_os = "linux"))]
        {
            let bytes = b"hidden".to_vec();
            match UnixAddr::from_abstract_name(&bytes) {
                Ok(addr) => assert_eq!(addr.as_abstract_name(), Some(&*bytes)),
                Err(err) => panic!("Unexpected error: {err}"),
            }

            // abstract socket name can contain NULL byte
            let bytes = b"hid\0den".to_vec();
            match UnixAddr::from_abstract_name(&bytes) {
                Ok(addr) => assert_eq!(addr.as_abstract_name(), Some(&*bytes)),
                Err(err) => panic!("Unexpected error: {err}"),
            }
        }

        let useless_socket = std::os::unix::net::UnixDatagram::unbound().unwrap();
        let unnamed_addr = useless_socket.local_addr().unwrap();

        match UnixAddr::from(unnamed_addr).address() {
            AddressKind::Unnamed => {}
            _ => panic!("Unexpected address kind"),
        }

        drop(useless_socket);
    }

    #[test]
    fn test_convert_unix_addr() {
        fn test(addr: &SocketAddr, must_be_of_type: AddressKind) {
            let unix_addr = UnixAddr::from(addr.clone());
            assert_eq!(
                unix_addr.address(),
                must_be_of_type,
                "{addr:?} must be of type {must_be_of_type:?}"
            );

            let socket_addr_from_unix = SocketAddr::from(unix_addr);
            match must_be_of_type {
                #[cfg(any(target_os = "android", target_os = "linux"))]
                AddressKind::Abstract(name) => {
                    assert_eq!(socket_addr_from_unix.as_abstract_name(), Some(name));
                }
                #[cfg(not(any(target_os = "android", target_os = "linux")))]
                AddressKind::Abstract(name) => unreachable!(),
                AddressKind::Pathname(path) => {
                    assert_eq!(socket_addr_from_unix.as_pathname(), Some(path));
                }
                AddressKind::Unnamed => {}
            }
        }

        let named_std_addr = SocketAddr::from_pathname("/tmp/sock").unwrap();
        let useless_socket = std::os::unix::net::UnixDatagram::unbound().unwrap();
        let unnamed_addr = useless_socket.local_addr().unwrap();
        #[cfg(any(target_os = "android", target_os = "linux"))]
        let abstract_addr = SocketAddr::from_abstract_name(b"hidden").unwrap();

        test(&named_std_addr, AddressKind::Pathname("/tmp/sock".as_ref()));
        test(&unnamed_addr, AddressKind::Unnamed);
        #[cfg(any(target_os = "android", target_os = "linux"))]
        test(&abstract_addr, AddressKind::Abstract(b"hidden"));

        drop(useless_socket);
    }
}
