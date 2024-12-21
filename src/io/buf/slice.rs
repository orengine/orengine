use crate::io::{Buffer, FixedBuffer, FixedBufferMut, SendableBuffer};
use crate::utils::Sealed;
use std::ops::{Deref, DerefMut};
use std::ptr;

macro_rules! impl_shared_slice {
    ($($ty:ty),*) => {
        $(
            impl<'buf> $ty {
                #[doc = "Returns what byte of the [`Buffer`] does the slice start from."]
                #[inline(always)]
                pub fn start(&self) -> u32 {
                    self.start
                }

                #[doc = "Returns what byte of the [`Buffer`] does the slice end at."]
                #[inline(always)]
                pub fn end(&self) -> u32 {
                    self.end
                }
            }

            impl Sealed for $ty {}

            impl FixedBuffer for $ty {
                #[inline(always)]
                fn as_ptr(&self) -> *const u8 {
                    unsafe { self.buf.as_ptr().offset(self.start as isize) }
                }

                #[inline(always)]
                fn len_u32(&self) -> u32 {
                    self.end - self.start
                }

                #[inline(always)]
                fn fixed_index(&self) -> u16 {
                    self.buf.fixed_index()
                }

                #[inline(always)]
                fn is_fixed(&self) -> bool {
                    self.buf.is_fixed()
                }
            }

        impl<'buf> Deref for $ty {
                type Target = [u8];

                #[inline(always)]
                fn deref(&self) -> &Self::Target {
                    unsafe { &*ptr::slice_from_raw_parts(self.as_ptr(), self.len_u32() as usize) }
                }
            }
        )*
    };
}

/// Represents an immutable slice of [`Buffer`].
///
// TODO example with read_exact
pub struct Slice<'buf> {
    buf: &'buf Buffer,
    start: u32,
    end: u32,
}

impl<'buf> Slice<'buf> {
    /// Creates a new [`Slice`].
    pub const fn new(buf: &'buf Buffer, start: u32, end: u32) -> Self {
        Self { buf, start, end }
    }
}

/// Represents a mutable slice of [`SliceMut`].
///
// TODO example
pub struct SliceMut<'buf> {
    buf: &'buf mut Buffer,
    start: u32,
    end: u32,
}

impl<'buf> SliceMut<'buf> {
    /// Creates a new [`SliceMut`].
    pub const fn new(buf: &'buf mut Buffer, start: u32, end: u32) -> Self {
        Self { buf, start, end }
    }
}

impl DerefMut for SliceMut<'_> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *ptr::slice_from_raw_parts_mut(self.as_mut_ptr(), self.len_u32() as usize) }
    }
}

impl FixedBufferMut for SliceMut<'_> {
    #[inline(always)]
    fn as_mut_ptr(&mut self) -> *mut u8 {
        unsafe { self.buf.as_mut_ptr().offset(self.start as isize) }
    }
}

impl_shared_slice! { Slice<'_>, SliceMut<'_> }

/// Represents an immutable slice of [`Buffer`].
///
// TODO example with read_exact
pub struct SendableSlice<'buf> {
    buf: &'buf SendableBuffer,
    start: u32,
    end: u32,
}

impl<'buf> SendableSlice<'buf> {
    /// Creates a new [`Slice`].
    pub const fn new(buf: &'buf SendableBuffer, start: u32, end: u32) -> Self {
        Self { buf, start, end }
    }
}

/// Represents a mutable slice of [`SliceMut`].
///
// TODO example
pub struct SendableSliceMut<'buf> {
    buf: &'buf mut SendableBuffer,
    start: u32,
    end: u32,
}

impl<'buf> SendableSliceMut<'buf> {
    /// Creates a new [`SliceMut`].
    pub const fn new(buf: &'buf mut SendableBuffer, start: u32, end: u32) -> Self {
        Self { buf, start, end }
    }
}

impl DerefMut for SendableSliceMut<'_> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *ptr::slice_from_raw_parts_mut(self.as_mut_ptr(), self.len_u32() as usize) }
    }
}

impl FixedBufferMut for SendableSliceMut<'_> {
    #[inline(always)]
    fn as_mut_ptr(&mut self) -> *mut u8 {
        unsafe { self.buf.as_mut_ptr().offset(self.start as isize) }
    }
}

impl_shared_slice! { SendableSlice<'_>, SendableSliceMut<'_> }

#[cfg(test)]
mod tests {
    use crate as orengine;
    use crate::io::buffer;

    #[orengine::test::test_local]
    fn test_slice() {
        let mut buf = buffer();
        buf.append(&[1, 2, 3, 4, 5]);

        let slice = buf.slice(..3);
        assert_eq!(slice.start(), 0);
        assert_eq!(slice.end(), 3);
        assert_eq!(slice.as_ref(), &[1, 2, 3]);

        let mut slice = buf.slice_mut(..3);
        slice[0] = 10;
        assert_eq!(slice.start(), 0);
        assert_eq!(slice.end(), 3);
        assert_eq!(slice.as_ref(), &[10, 2, 3]);

        let slice = buf.slice(3..);
        assert_eq!(slice.start(), 3);
        assert_eq!(slice.end(), 5);
        assert_eq!(slice.as_ref(), &[4, 5]);

        let mut slice = buf.slice_mut(3..);
        slice[0] = 40;
        assert_eq!(slice.start(), 3);
        assert_eq!(slice.end(), 5);
        assert_eq!(slice.as_ref(), &[40, 5]);

        let slice = buf.slice(..);
        assert_eq!(slice.start(), 0);
        assert_eq!(slice.end(), 5);
        assert_eq!(slice.as_ref(), &[10, 2, 3, 40, 5]);

        let mut slice = buf.slice_mut(..);
        slice[2] = 30;
        assert_eq!(slice.start(), 0);
        assert_eq!(slice.end(), 5);
        assert_eq!(slice.as_ref(), &[10, 2, 30, 40, 5]);

        let slice = buf.slice(1..=1);
        assert_eq!(slice.as_ref(), &[2]);

        let mut slice = buf.slice_mut(1..=1);
        slice[0] = 20;
        assert_eq!(slice.start(), 1);
        assert_eq!(slice.end(), 2);
        assert_eq!(slice.as_ref(), &[20]);

        assert_eq!(buf.as_ref(), &[10, 20, 30, 40, 5]);
    }
}
