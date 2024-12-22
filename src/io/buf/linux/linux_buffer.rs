use std::alloc::{alloc, Layout};
use std::marker::PhantomData;
use std::ptr;

#[repr(align(32))]
pub(crate) struct FixedBuffer {
    len: u32,
    // cap is a default buffer capacity of pool. But we cache it here to avoid calling thread_static
    cap: u32,
    index: u16,
    ptr: *mut u8,
    // impl !Send
    no_send_marker: PhantomData<*const ()>,
}

impl FixedBuffer {
    /// Returns the index associated with the buffer.
    #[inline(always)]
    pub(crate) fn index(&self) -> u16 {
        self.index
    }
}

pub(crate) enum LinuxBuffer {
    Fixed(FixedBuffer),
    NonFixed(Vec<u8>),
}

impl LinuxBuffer {
    pub(crate) fn new_fixed(ptr: *mut u8, cap: u32, index: u16) -> Self {
        Self::Fixed(FixedBuffer {
            ptr,
            len: 0,
            cap,
            index,
            no_send_marker: PhantomData,
        })
    }

    #[inline(always)]
    pub(crate) fn new_non_fixed(size: u32) -> Self {
        debug_assert!(
            size > 0,
            "Cannot create Buffer with size 0. Size must be > 0."
        );

        let layout = Layout::array::<u8>(size as _).unwrap_or_else(|_| {
            panic!("Cannot create slice with capacity {size}. Capacity overflow.")
        });

        Self::NonFixed(unsafe { Vec::from_raw_parts(alloc(layout), 0, size as _) })
    }
    #[inline(always)]
    pub(crate) fn len(&self) -> u32 {
        match self {
            Self::Fixed(f) => f.len,
            #[allow(clippy::cast_possible_truncation, reason = "we have to cast it")]
            Self::NonFixed(v) => v.len() as u32,
        }
    }

    /// # Safety
    ///
    /// `len` must be less than or equal to `capacity`.
    #[inline(always)]
    pub(crate) unsafe fn set_len(&mut self, len: u32) {
        match self {
            Self::Fixed(f) => f.len = len,
            Self::NonFixed(v) => unsafe { v.set_len(len as usize) },
        }
    }

    #[inline(always)]
    pub(crate) fn capacity(&self) -> u32 {
        match self {
            Self::Fixed(f) => f.cap,
            #[allow(clippy::cast_possible_truncation, reason = "we have to cast it")]
            Self::NonFixed(v) => v.capacity() as u32,
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
            Self::Fixed(f) => unsafe { &*ptr::slice_from_raw_parts(f.ptr, f.len as usize) },
            Self::NonFixed(v) => v.as_ref(),
        }
    }
}

impl AsMut<[u8]> for LinuxBuffer {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut [u8] {
        match self {
            Self::Fixed(f) => unsafe { &mut *ptr::slice_from_raw_parts_mut(f.ptr, f.len as usize) },
            Self::NonFixed(v) => v.as_mut(),
        }
    }
}
