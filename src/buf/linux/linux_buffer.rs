use crate::buf::buf_pool::BufPool;
use std::alloc::{alloc, Layout};
use std::ptr;

pub(crate) struct FixedBuffer {
    ptr: *mut u8,
    len: usize,
    // cap is a default buffer capacity of pool. But we cache it here to avoid calling thread_static
    cap: usize,
    index: usize,
}

pub(crate) enum LinuxBuffer {
    Fixed(FixedBuffer),
    NonFixed(Vec<u8>),
}

impl LinuxBuffer {
    pub(crate) fn new_fixed(ptr: *mut u8, cap: usize, index: usize) -> Self {
        Self::Fixed(FixedBuffer {
            ptr,
            len: 0,
            cap,
            index,
        })
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
        Self::new_non_fixed(pool.default_buffer_capacity())
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Fixed(f) => f.len,
            Self::NonFixed(v) => v.len(),
        }
    }

    /// # Safety
    ///
    /// `len` must be less than or equal to `capacity`.
    #[inline(always)]
    pub(crate) unsafe fn set_len(&mut self, len: usize) {
        match self {
            Self::Fixed(f) => f.len = len,
            Self::NonFixed(v) => unsafe { v.set_len(len) },
        }
    }

    #[inline(always)]
    pub(crate) fn capacity(&self) -> usize {
        match self {
            Self::Fixed(f) => f.cap,
            Self::NonFixed(v) => v.capacity(),
        }
    }

    #[inline(always)]
    pub(crate) fn as_ptr(&self) -> *const u8 {
        match self {
            Self::Fixed(f) => f.ptr,
            Self::NonFixed(v) => v.as_ptr(),
        }
    }

    #[inline(always)]
    pub(crate) fn as_mut_ptr(&mut self) -> *mut u8 {
        match self {
            Self::Fixed(f) => f.ptr,
            Self::NonFixed(v) => v.as_mut_ptr(),
        }
    }
}

impl AsRef<[u8]> for LinuxBuffer {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Fixed(f) => unsafe { &*ptr::slice_from_raw_parts(f.ptr, f.len) },
            Self::NonFixed(v) => v.as_ref(),
        }
    }
}

impl AsMut<[u8]> for LinuxBuffer {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut [u8] {
        match self {
            Self::Fixed(f) => unsafe { &mut *ptr::slice_from_raw_parts_mut(f.ptr, f.len) },
            Self::NonFixed(v) => v.as_mut(),
        }
    }
}
