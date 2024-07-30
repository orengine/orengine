use std::io::IoSlice;
use std::mem;
use std::net::SocketAddr;
use nix::libc;
use socket2::SockAddr;

pub(crate) type OsMessageHeader = libc::msghdr;

pub(crate) struct MessageHeader<'buf> {
    header: OsMessageHeader,
    io_slices: Vec<IoSlice<'buf>>,
    sock_addr: SockAddr
}

impl<'buf> MessageHeader<'buf> {
    /// Used in RecvFrom. So we need to use `*mut [u8]`.
    #[inline(always)]
    pub(crate) fn new_for_recv_from(buf_ref: &'buf mut [u8]) -> Self {
        let mut s = MessageHeader {
            header: unsafe { mem::zeroed() },
            io_slices: vec![IoSlice::new(buf_ref)],
            sock_addr: unsafe { mem::zeroed() }
        };

        s.header.msg_iov = s.io_slices.as_mut_ptr() as _;
        s.header.msg_iovlen = s.io_slices.len();

        s
    }

    /// Used in SendTo. So we need to use `*const [u8]` and [`SocketAddr`].
    #[inline(always)]
    pub(crate) fn new_for_send_to(buf_ref: &'buf [u8], addr: SocketAddr) -> Self {
        let mut s = MessageHeader {
            header: unsafe { mem::zeroed() },
            io_slices: vec![IoSlice::new(buf_ref)],
            sock_addr: SockAddr::from(addr)
        };

        s.header.msg_iov = s.io_slices.as_mut_ptr() as _;
        s.header.msg_iovlen = s.io_slices.len();

        s
    }

    #[inline(always)]
    pub(crate) fn get_os_message_header_ptr(&mut self) -> *mut OsMessageHeader {
        self.header.msg_name = self.sock_addr.as_ptr() as _;
        self.header.msg_namelen = mem::size_of::<SockAddr>() as _;
        &mut self.header
    }

    #[inline(always)]
    pub(crate) fn socket_addr(&self) -> &SockAddr {
        &self.sock_addr
    }
}