use std::io::IoSlice;
use std::mem;
use std::mem::size_of;

use nix::libc;
use socket2::SockAddr;

pub(crate) type OsMessageHeader = libc::msghdr;

pub(crate) struct MessageRecvHeader<'header> {
    header: OsMessageHeader,
    io_slices: Vec<IoSlice<'header>>,
}

impl<'header> MessageRecvHeader<'header> {
    #[inline(always)]
    pub(crate) fn new(buf_ref: &'header mut [u8]) -> Self {
        let mut s = MessageRecvHeader {
            header: unsafe { mem::zeroed() },
            io_slices: vec![IoSlice::new(buf_ref)],
        };

        s.header.msg_iov = s.io_slices.as_mut_ptr() as _;
        s.header.msg_iovlen = s.io_slices.len();

        s
    }

    #[inline(always)]
    pub(crate) fn get_os_message_header_ptr(&mut self, sock_addr: *mut SockAddr) -> *mut OsMessageHeader {
        self.header.msg_name = sock_addr as _;
        self.header.msg_namelen = size_of::<SockAddr>() as _;
        &mut self.header
    }
}

pub(crate) struct MessageSendHeader<'header> {
    header: OsMessageHeader,
    io_slices: Vec<IoSlice<'header>>,
}

impl<'header> MessageSendHeader<'header> {
    /// Used in SendTo. So we need to use `*const [u8]` and [`SocketAddr`].
    #[inline(always)]
    pub(crate) fn new(buf_ref: &'header [u8]) -> Self {
        let mut s = MessageSendHeader {
            header: unsafe { mem::zeroed() },
            io_slices: vec![IoSlice::new(buf_ref)],
        };

        s.header.msg_iov = s.io_slices.as_mut_ptr() as _;
        s.header.msg_iovlen = s.io_slices.len();

        s
    }

    #[inline(always)]
    pub(crate) fn get_os_message_header_ptr(&mut self, addr: &'header SockAddr) -> *mut OsMessageHeader {
        self.header.msg_name = addr.as_ptr() as _;
        self.header.msg_namelen = addr.len() as _;
        &mut self.header
    }
}
