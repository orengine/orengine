#[macro_use]
mod unix_impl_socket {
    /// This macro creates a `Socket` trait implementation for Unix sockets.
    macro_rules! unix_impl_socket {
        () => {
            type Addr = crate::net::unix::UnixAddr;

            #[inline]
            fn is_unix(&self) -> bool {
                true
            }
        };
    }

    pub(crate) use unix_impl_socket;
}

pub(crate) use unix_impl_socket::unix_impl_socket;
