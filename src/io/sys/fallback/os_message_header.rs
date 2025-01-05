use socket2::SockAddr;
use std::ptr;

/// Synonymous with os message header.
pub(crate) type OsMessageHeader = (*mut *mut [u8], *mut SockAddr);

/// [`MessageRecvHeader`] keeps the message header for `recvfrom`.
pub(crate) struct MessageRecvHeader {
    os_header: OsMessageHeader,
}

impl MessageRecvHeader {
    /// Creates a new [`MessageRecvHeader`].
    pub(crate) fn new(addr: *mut SockAddr, buf_ptr: *mut *mut [u8]) -> Self {
        Self {
            os_header: (buf_ptr, addr),
        }
    }

    /// Returns a shared reference to the message header.
    #[inline(always)]
    pub(crate) fn get_os_message_header(&self) -> &OsMessageHeader {
        &self.os_header
    }
}

/// [`MessageSendHeader`] keeps the message header for `sendto`.
pub(crate) struct MessageSendHeader {
    os_header: OsMessageHeader,
}

impl MessageSendHeader {
    /// Creates a new [`MessageSendHeader`].
    #[inline(always)]
    pub(crate) fn new() -> Self {
        Self {
            os_header: (ptr::null_mut(), ptr::null_mut()),
        }
    }

    /// Initializes the message header.
    #[inline(always)]
    pub(crate) fn init(&mut self, addr: &SockAddr, buf_ref: *mut *const [u8]) {
        self.os_header.0 = buf_ref.cast();
        self.os_header.1 = ptr::from_ref(addr).cast_mut();
    }

    /// Returns a pointer to the message header after its initialization.
    #[inline(always)]
    pub(crate) fn get_os_message_header_ptr(
        &mut self,
        addr: &SockAddr,
        buf_ref: *mut *const [u8],
    ) -> *mut OsMessageHeader {
        self.init(addr, buf_ref);

        ptr::from_mut(&mut self.os_header)
    }
}
