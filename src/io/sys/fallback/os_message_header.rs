use socket2::SockAddr;
use std::io::{IoSlice, IoSliceMut};
use std::ptr;

/// Synonymous with os message header.
pub(crate) type OsMessageHeader = (*mut [IoSliceMut<'static>], *mut SockAddr);

/// [`MessageRecvHeader`] keeps the message header for `recvfrom`.
pub(crate) struct MessageRecvHeader {
    os_header: OsMessageHeader,
}

impl MessageRecvHeader {
    /// Creates a new [`MessageRecvHeader`].
    pub(crate) fn new(addr: *mut SockAddr, buf_ptr: *mut [IoSliceMut<'_>]) -> Self {
        #[allow(clippy::unnecessary_cast, reason = "False positive.")]
        Self {
            os_header: (buf_ptr as *mut [IoSliceMut<'static>], addr),
        }
    }

    /// Returns a length of an associated addr.
    #[inline(always)]
    pub(crate) fn get_addr_len(&self) -> crate::io::sys::socklen_t {
        unsafe { &*self.os_header.1 }.len()
    }

    /// Returns a shared reference to the message header.
    #[inline(always)]
    pub(crate) fn get_os_message_header(&mut self) -> &OsMessageHeader {
        &mut self.os_header
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
            os_header: (
                ptr::slice_from_raw_parts_mut(ptr::null_mut(), 0),
                ptr::null_mut(),
            ),
        }
    }

    /// Initializes the message header.
    #[inline(always)]
    pub(crate) fn init(&mut self, addr: &SockAddr, buf_ref: *mut [IoSlice<'_>]) {
        self.os_header.0 = buf_ref as *mut [IoSliceMut<'static>];
        self.os_header.1 = ptr::from_ref(addr).cast_mut();
    }

    /// Returns a pointer to the message header after its initialization.
    #[inline(always)]
    pub(crate) fn get_os_message_header_ptr(
        &mut self,
        addr: &SockAddr,
        buf_ref: *const [IoSlice],
    ) -> *mut OsMessageHeader {
        self.init(addr, buf_ref.cast_mut());

        ptr::from_mut(&mut self.os_header)
    }
}
