use crate::buf::buf_pool::{buf_pool, BufPool};
use nix::libc::iovec;
use std::alloc::{alloc, Layout};
use std::ptr;

pub(crate) struct FixedBuffer {
    iovec: iovec,
    index: usize,
}

pub(crate) enum LinuxBuffer {
    Fixed(FixedBuffer),
    NonFixed(Vec<u8>),
}

impl LinuxBuffer {
    pub(crate) fn new_fixed(vec: iovec, index: usize) -> Self {
        Self::Fixed(FixedBuffer { iovec: vec, index })
    }

    #[inline(always)]
    pub(crate) fn new_non_fixed(size: usize) -> Self {
        debug_assert!(
            size > 0,
            "Cannot create Buffer with size 0. Size must be > 0."
        );

        let layout = Layout::array::<u8>(size).expect(&format!(
            "Cannot create slice with capacity {size}. Capacity overflow."
        ));

        Self::NonFixed(unsafe { Vec::from_raw_parts(alloc(layout), 0, size) })
    }

    /// Creates a new buffer from a pool with the given size.
    #[inline(always)]
    pub(crate) fn new_non_fixed_from_pool(pool: &BufPool) -> Self {
        Self::new_non_fixed(pool.default_buffer_cap())
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Fixed(f) => f.iovec.iov_len,
            Self::NonFixed(v) => v.len(),
        }
    }

    /// # Safety
    ///
    /// `len` must be less than or equal to `capacity`.
    #[inline(always)]
    pub(crate) unsafe fn set_len(&mut self, len: usize) {
        match self {
            Self::Fixed(f) => f.iovec.iov_len = len,
            Self::NonFixed(v) => unsafe { v.set_len(len) },
        }
    }

    #[inline(always)]
    pub(crate) fn capacity(&self) -> usize {
        match self {
            Self::Fixed(_) => buf_pool().default_buffer_cap(),
            Self::NonFixed(v) => v.capacity(),
        }
    }

    #[inline(always)]
    pub(crate) fn as_ptr(&self) -> *const u8 {
        match self {
            Self::Fixed(f) => f.iovec.iov_base.cast(),
            Self::NonFixed(v) => v.as_ptr(),
        }
    }

    #[inline(always)]
    pub(crate) fn as_mut_ptr(&mut self) -> *mut u8 {
        match self {
            Self::Fixed(f) => f.iovec.iov_base.cast(),
            Self::NonFixed(v) => v.as_mut_ptr(),
        }
    }
}

impl AsRef<[u8]> for LinuxBuffer {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Fixed(f) => unsafe {
                &*ptr::slice_from_raw_parts(f.iovec.iov_base.cast(), f.iovec.iov_len)
            },
            Self::NonFixed(v) => v.as_ref(),
        }
    }
}

impl AsMut<[u8]> for LinuxBuffer {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut [u8] {
        match self {
            Self::Fixed(f) => unsafe {
                &mut *ptr::slice_from_raw_parts_mut(f.iovec.iov_base.cast(), f.iovec.iov_len)
            },
            Self::NonFixed(v) => v.as_mut(),
        }
    }
}
