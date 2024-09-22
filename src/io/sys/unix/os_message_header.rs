use std::marker::PhantomData;
use std::mem;
use std::mem::size_of;

use nix::libc;
use socket2::SockAddr;

/// Synonymous with os message header.
pub(crate) type OsMessageHeader = libc::msghdr;

/// [`MessageRecvHeader`] keeps the message header for `recvfrom`.
pub(crate) struct MessageRecvHeader<'header> {
    header: OsMessageHeader,
    phantom_data: PhantomData<&'header [u8]>
}

impl<'header> MessageRecvHeader<'header> {
    /// Creates a new [`MessageRecvHeader`].
    pub(crate) fn new() -> Self {
        unsafe { mem::zeroed() }
    }

    /// Initializes the message header.
    #[inline(always)]
    pub(crate) fn init(&mut self, addr: *mut SockAddr, buf_ptr: &mut *mut [u8]) {
        self.header.msg_name = addr as _;
        self.header.msg_namelen = size_of::<SockAddr>() as _;

        self.header.msg_iov = buf_ptr as *mut _ as _;
        self.header.msg_iovlen = 1;
    }

    /// Returns a pointer to the message header after its initialization.
    #[inline(always)]
    pub(crate) fn get_os_message_header_ptr(
        &mut self,
        sock_addr: *mut SockAddr,
        buf_ptr: &mut *mut [u8]
    ) -> *mut OsMessageHeader {
        self.init(sock_addr, buf_ptr);

        &mut self.header
    }
}

/// [`MessageSendHeader`] keeps the message header for `sendto`.
pub(crate) struct MessageSendHeader<'header> {
    header: OsMessageHeader,
    phantom_data: PhantomData<&'header [u8]>
}

impl<'header> MessageSendHeader<'header> {
    /// Creates a new [`MessageSendHeader`].
    #[inline(always)]
    pub(crate) fn new() -> Self {
        let s = MessageSendHeader {
            header: unsafe { mem::zeroed() },
            phantom_data: PhantomData
        };
        s
    }

    /// Initializes the message header.
    #[inline(always)]
    pub(crate) fn init(&mut self, addr: &'header SockAddr, buf_ref: *mut *const [u8]) {
        self.header.msg_name = addr.as_ptr() as _;
        self.header.msg_namelen = addr.len() as _;

        self.header.msg_iov = buf_ref as _;
        self.header.msg_iovlen = 1;
    }

    /// Returns a pointer to the message header after its initialization.
    #[inline(always)]
    pub(crate) fn get_os_message_header_ptr(
        &mut self,
        addr: &'header SockAddr,
        buf_ref: *mut *const [u8]
    ) -> *mut OsMessageHeader {
        self.init(addr, buf_ref);

        &mut self.header
    }
}
