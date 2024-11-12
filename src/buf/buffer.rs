use std::alloc::{alloc, dealloc, Layout};
use std::cmp::max;
use std::fmt::Debug;
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::ptr;
use std::ptr::{slice_from_raw_parts_mut, NonNull};
use std::slice::SliceIndex;

use crate::buf::buf_pool::buf_pool;
use crate::buf::buffer;

/// Buffer for data transfer. Buffer is allocated in heap.
///
/// Buffer has `len` and `cap`.
///
/// - `len` is how many bytes have been written into the buffer.
/// - `cap` is how many bytes have been allocated for the buffer.
///
/// # About pool
///
/// For get from [`BufPool`](crate::buf::BufPool), call [`buffer`](crate::buf::buffer)
/// or [`full_buffer`](crate::buf::full_buffer).
/// If you can use [`BufPool`], use it, to have better performance.
///
/// If it was gotten from [`BufPool`] it will come back after drop.
///
/// # Buffer representation
///
/// ```text
/// +---+---+---+---+---+---+---+---+
/// | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
/// +---+---+---+---+---+---+---+---+
/// | X | X | X | X | X |   |   |   |
/// +---+---+---+---+---+---+---+---+
///                     ^           ^
///                    len         cap
///
/// len = 5 (from 1 to 5 inclusive)
/// 5 blocks occupied (X), 3 blocks free (blank)
/// ```
pub struct Buffer {
    pub(crate) slice: NonNull<[u8]>,
    len: usize,
}

impl Buffer {
    /// Creates raw slice with given capacity. This code avoids checking zero capacity.
    ///
    /// # Safety
    ///
    /// capacity > 0
    #[inline(always)]
    fn raw_slice(capacity: usize) -> NonNull<[u8]> {
        let layout = Layout::array::<u8>(capacity)
            .expect("Cannot create slice with capacity {capacity}. Capacity overflow.");
        unsafe { NonNull::new_unchecked(slice_from_raw_parts_mut(alloc(layout), capacity)) }
    }

    /// Creates new buffer with given size. This buffer will not be put to the pool.
    /// So, use it only for creating a buffer with specific size.
    ///
    /// # Safety
    ///
    /// - size > 0
    #[inline(always)]
    pub fn new(size: usize) -> Self {
        debug_assert!(
            size > 0,
            "Cannot create Buffer with size 0. Size must be > 0."
        );

        Self {
            slice: Self::raw_slice(size),
            len: 0,
        }
    }

    /// Creates a new buffer from a pool with the given size.
    #[inline(always)]
    pub(crate) fn new_from_pool(size: usize) -> Self {
        Self {
            slice: Self::raw_slice(size),
            len: 0,
        }
    }

    /// Returns `true` if the buffer is empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns how many bytes have been written into the buffer.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Increases [`len`](#field.len) by the diff.
    #[inline(always)]
    pub fn add_len(&mut self, diff: usize) {
        self.len += diff;
    }

    /// Sets [`len`](#field.len).
    #[inline(always)]
    pub fn set_len(&mut self, len: usize) {
        self.len = len;
    }

    /// Sets [`len`](#field.len) to [`real_cap`](#method.real_cap).
    #[inline(always)]
    pub fn set_len_to_cap(&mut self) {
        self.len = self.cap();
    }

    /// Returns a real capacity of the buffer.
    #[inline(always)]
    pub fn cap(&self) -> usize {
        self.slice.len()
    }

    /// Returns `true` if the buffer is full.
    #[inline(always)]
    pub fn is_full(&self) -> bool {
        self.cap() == self.len
    }

    /// Resizes the buffer to a new size.
    ///
    /// If the `new_size` is less than the current length of the buffer,
    /// the length is truncated to `new_size`.
    ///
    /// A new buffer is created with the specified `new_size`.
    /// If a buffer of the same size is available in the buffer pool, it is reused;
    /// otherwise, a new buffer is allocated.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::buf::Buffer;
    ///
    /// let mut buf = Buffer::new(100);
    /// buf.append(&[1, 2, 3]);
    /// buf.resize(200); // Buffer is resized to 200, contents are preserved
    /// assert_eq!(buf.cap(), 200);
    /// assert_eq!(buf.as_ref(), &[1, 2, 3]);
    /// ```
    #[inline(always)]
    pub fn resize(&mut self, new_size: usize) {
        if new_size < self.len {
            self.len = new_size;
        }

        let mut new_buf = if buf_pool().buffer_len() == new_size {
            buffer()
        } else {
            Self::new(new_size)
        };

        new_buf.len = self.len;
        unsafe {
            ptr::copy_nonoverlapping(self.as_ptr(), new_buf.as_mut_ptr(), self.len);
        }

        *self = new_buf;
    }

    /// Appends data to the buffer.
    /// If a capacity is not enough, the buffer will be resized.
    #[inline(always)]
    pub fn append(&mut self, buf: &[u8]) {
        let len = buf.len();
        if len > self.slice.len() - self.len {
            self.resize(max(self.len + len, self.cap() * 2));
        }

        unsafe {
            ptr::copy_nonoverlapping(buf.as_ptr(), self.as_mut_ptr().add(self.len), len);
        }
        self.len += len;
    }

    /// Returns a pointer to the buffer.
    #[inline(always)]
    pub fn as_ptr(&self) -> *const u8 {
        self.slice.as_ptr().cast()
    }

    /// Returns a mutable pointer to the buffer.
    #[inline(always)]
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.slice.as_ptr().cast()
    }

    /// Clears the buffer.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Puts the buffer to the pool. You can not to use it, and then this method will be called automatically by drop.
    #[inline(always)]
    pub fn release(self) {
        buf_pool().put(self);
    }

    /// Puts the buffer to the pool without checking for a size.
    ///
    /// # Safety
    /// - [`buf.real_cap`](#method.real_cap)() == [`config_buf_len`](config_buf_len)()
    #[inline(always)]
    pub unsafe fn release_unchecked(self) {
        unsafe { buf_pool().put_unchecked(self) };
    }
}

impl Deref for Buffer {
    type Target = [u8];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl DerefMut for Buffer {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut()
    }
}

impl<I: SliceIndex<[u8]>> Index<I> for Buffer {
    type Output = I::Output;

    #[inline(always)]
    fn index(&self, index: I) -> &Self::Output {
        self.as_ref().index(index)
    }
}

impl<I: SliceIndex<[u8]>> IndexMut<I> for Buffer {
    #[inline(always)]
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        self.as_mut().index_mut(index)
    }
}

impl AsRef<[u8]> for Buffer {
    fn as_ref(&self) -> &[u8] {
        unsafe { &self.slice.as_ref()[0..self.len] }
    }
}

impl AsMut<[u8]> for Buffer {
    fn as_mut(&mut self) -> &mut [u8] {
        unsafe { &mut self.slice.as_mut()[0..self.len] }
    }
}

impl PartialEq<&[u8]> for Buffer {
    fn eq(&self, other: &&[u8]) -> bool {
        self.as_ref() == *other
    }
}

impl Debug for Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.as_ref())
    }
}

impl From<Box<[u8]>> for Buffer {
    fn from(slice: Box<[u8]>) -> Self {
        Self {
            len: slice.len(),
            slice: NonNull::from(Box::leak(slice)),
        }
    }
}

impl<const N: usize> From<Box<[u8; N]>> for Buffer {
    fn from(slice: Box<[u8; N]>) -> Self {
        Self {
            len: slice.len(),
            slice: NonNull::from(Box::leak(slice)),
        }
    }
}

impl From<Vec<u8>> for Buffer {
    fn from(mut slice: Vec<u8>) -> Self {
        let l = slice.len();
        unsafe { slice.set_len(slice.capacity()) }

        Self {
            len: l,
            slice: NonNull::from(slice.leak()),
        }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        let pool = buf_pool();
        if self.cap() == pool.buffer_len() {
            let buf = Self {
                slice: self.slice,
                len: self.len,
            };
            unsafe { pool.put_unchecked(buf) };
        } else {
            unsafe {
                dealloc(
                    self.slice.as_ptr().cast(),
                    Layout::array::<u8>(self.cap()).unwrap_unchecked(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;

    #[orengine_macros::test_local]
    fn test_new() {
        let buf = Buffer::new(1);
        assert_eq!(buf.cap(), 1);
    }

    #[orengine_macros::test_local]
    fn test_add_len_and_set_len_to_cap() {
        let mut buf = Buffer::new(100);
        assert_eq!(buf.len(), 0);

        buf.add_len(10);
        assert_eq!(buf.len(), 10);

        buf.add_len(20);
        assert_eq!(buf.len(), 30);

        buf.set_len_to_cap();
        assert_eq!(buf.len(), buf.cap());
    }

    #[orengine_macros::test_local]
    fn test_len_and_cap() {
        let mut buf = Buffer::new(100);
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.cap(), 100);

        buf.set_len(10);
        assert_eq!(buf.len(), 10);
        assert_eq!(buf.cap(), 100);
    }

    #[orengine_macros::test_local]
    fn test_resize() {
        let mut buf = Buffer::new(100);
        buf.append(&[1, 2, 3]);

        buf.resize(200);
        assert_eq!(buf.cap(), 200);
        assert_eq!(buf.as_ref(), &[1, 2, 3]);

        buf.resize(50);
        assert_eq!(buf.cap(), 50);
        assert_eq!(buf.as_ref(), &[1, 2, 3]);
    }

    #[orengine_macros::test_local]
    fn test_append_and_clear() {
        let mut buf = Buffer::new(5);

        buf.append(&[1, 2, 3]);
        // This code checks written
        assert_eq!(buf.as_ref(), &[1, 2, 3]);
        assert_eq!(buf.cap(), 5);

        buf.append(&[4, 5, 6]);
        assert_eq!(buf.as_ref(), &[1, 2, 3, 4, 5, 6]);
        assert_eq!(buf.cap(), 10);

        buf.clear();
        assert_eq!(buf.as_ref(), &[]);
        assert_eq!(buf.cap(), 10);
    }

    #[orengine_macros::test_local]
    fn test_is_empty_and_is_full() {
        let mut buf = Buffer::new(5);
        assert!(buf.is_empty());
        buf.append(&[1, 2, 3]);
        assert!(!buf.is_empty());
        buf.clear();
        assert!(buf.is_empty());

        let mut buf = Buffer::new(5);
        assert!(!buf.is_full());
        buf.append(&[1, 2, 3, 4, 5]);
        assert!(buf.is_full());
        buf.clear();
        assert!(!buf.is_full());
    }

    #[orengine_macros::test_local]
    fn test_index() {
        let mut buf = Buffer::new(5);
        buf.append(&[1, 2, 3]);
        assert_eq!(buf[0], 1);
        assert_eq!(buf[1], 2);
        assert_eq!(buf[2], 3);
        assert_eq!(&buf[1..=2], &[2, 3]);
        assert_eq!(&buf[..3], &[1, 2, 3]);
        assert_eq!(&buf[2..], &[3]);
        assert_eq!(&buf[..], &[1, 2, 3]);
    }

    #[orengine_macros::test_local]
    fn test_from() {
        let b = Box::new([1, 2, 3]);
        let buf = Buffer::from(b);
        assert_eq!(buf.as_ref(), &[1, 2, 3]);
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.cap(), 3);

        let mut v = vec![1, 2, 3];
        v.reserve(7);
        let buf = Buffer::from(v);
        assert_eq!(buf.as_ref(), &[1, 2, 3]);
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.cap(), 10);

        let v = vec![1, 2, 3];
        let buf = Buffer::from(v.into_boxed_slice());
        assert_eq!(buf.as_ref(), &[1, 2, 3]);
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.cap(), 3);
    }
}
