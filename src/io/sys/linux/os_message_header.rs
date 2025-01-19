use std::io::{IoSlice, IoSliceMut};
use std::mem;
use std::mem::size_of;

use libc;
use socket2::SockAddr;

/// Synonymous with os message header.
pub(crate) type OsMessageHeader = libc::msghdr;

/// [`MessageRecvHeader`] keeps the message header for `recvfrom`.
pub(crate) struct MessageRecvHeader {
    os_header: OsMessageHeader,
}

impl MessageRecvHeader {
    /// Creates a new [`MessageRecvHeader`].
    pub(crate) fn new(addr: *mut SockAddr, buf_ptr: *mut [IoSliceMut]) -> Self {
        let mut s: Self = unsafe { mem::zeroed() };

        s.os_header.msg_name = addr.cast();

        #[allow(clippy::cast_possible_truncation, reason = "We can't process it here")]
        {
            s.os_header.msg_namelen = size_of::<SockAddr>() as _;
        }

        s.os_header.msg_iov = buf_ptr.cast::<libc::iovec>();
        s.os_header.msg_iovlen = buf_ptr.len() as _;

        s
    }

    /// Returns a shared reference to the message header.
    #[inline]
    pub(crate) fn get_os_message_header(&mut self) -> &mut OsMessageHeader {
        &mut self.os_header
    }

    /// Returns a length of an associated addr.
    #[inline]
    pub(crate) fn get_addr_len(&self) -> libc::socklen_t {
        self.os_header.msg_namelen
    }
}

/// [`MessageSendHeader`] keeps the message header for `sendto`.
pub(crate) struct MessageSendHeader {
    header: OsMessageHeader,
}

impl MessageSendHeader {
    /// Creates a new [`MessageSendHeader`].
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            header: unsafe { mem::zeroed() },
        }
    }

    /// Initializes the message header.
    #[inline]
    pub(crate) fn init(&mut self, addr: &SockAddr, buf_ref: *const [IoSlice]) {
        self.header.msg_name = addr.as_ptr().cast_mut().cast::<libc::c_void>();
        self.header.msg_namelen = addr.len() as _;

        self.header.msg_iov = buf_ref.cast::<libc::iovec>().cast_mut();
        self.header.msg_iovlen = buf_ref.len() as _;
    }

    /// Returns a pointer to the message header after its initialization.
    #[inline]
    pub(crate) fn get_os_message_header_ptr(
        &mut self,
        addr: &SockAddr,
        buf_ref: *const [IoSlice],
    ) -> *mut OsMessageHeader {
        self.init(addr, buf_ref);

        &mut self.header
    }
}
