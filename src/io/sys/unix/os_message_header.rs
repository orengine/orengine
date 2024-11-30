use std::marker::PhantomData;
use std::mem;
use std::mem::size_of;

use nix::libc;
use socket2::SockAddr;

/// Synonymous with os message header.
pub(crate) type OsMessageHeader = libc::msghdr;

/// [`MessageRecvHeader`] keeps the message header for `recvfrom`.
pub(crate) struct MessageRecvHeader<'header> {
    pub(crate) header: OsMessageHeader,
    phantom_data: PhantomData<&'header [u8]>,
}

impl MessageRecvHeader<'_> {
    /// Creates a new [`MessageRecvHeader`].
    pub(crate) fn new(addr: *mut SockAddr, buf_ptr: *mut *mut [u8]) -> Self {
        let mut s: Self = unsafe { mem::zeroed() };

        s.header.msg_name = addr.cast();

        #[allow(clippy::cast_possible_truncation, reason = "We can't process it here")]
        {
            s.header.msg_namelen = size_of::<SockAddr>() as _;
        }

        s.header.msg_iov = buf_ptr.cast::<libc::iovec>();
        s.header.msg_iovlen = 1;

        s
    }
}

/// [`MessageSendHeader`] keeps the message header for `sendto`.
pub(crate) struct MessageSendHeader<'header> {
    header: OsMessageHeader,
    phantom_data: PhantomData<&'header [u8]>,
}

impl<'header> MessageSendHeader<'header> {
    /// Creates a new [`MessageSendHeader`].
    #[inline(always)]
    pub(crate) fn new() -> Self {
        Self {
            header: unsafe { mem::zeroed() },
            phantom_data: PhantomData,
        }
    }

    /// Initializes the message header.
    #[inline(always)]
    pub(crate) fn init(&mut self, addr: &'header SockAddr, buf_ref: *mut *const [u8]) {
        self.header.msg_name = addr.as_ptr().cast_mut().cast::<libc::c_void>();
        self.header.msg_namelen = addr.len() as _;

        self.header.msg_iov = buf_ref.cast();
        self.header.msg_iovlen = 1;
    }

    /// Returns a pointer to the message header after its initialization.
    #[inline(always)]
    pub(crate) fn get_os_message_header_ptr(
        &mut self,
        addr: &'header SockAddr,
        buf_ref: *mut *const [u8],
    ) -> *mut OsMessageHeader {
        self.init(addr, buf_ref);

        &mut self.header
    }
}
