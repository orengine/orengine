use std::io;

/// Returns an [`unsupported by unix socket`](io::ErrorKind::Unsupported) error.
#[must_use]
#[cold]
pub(crate) fn new_unix_unsupported_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "Operation not supported by UNIX domain sockets",
    )
}
