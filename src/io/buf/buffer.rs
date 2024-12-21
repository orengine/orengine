use crate::io::buf_pool::{buf_pool, buffer, BufPool};
use crate::io::linux::linux_buffer::LinuxBuffer;
use crate::io::slice::{Slice, SliceMut};
use crate::io::{FixedBuffer, FixedBufferMut};
use crate::utils::Sealed;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::ops::{Bound, Deref, DerefMut, Index, IndexMut, RangeBounds};
use std::slice::SliceIndex;
use std::{fmt, mem, ptr};

#[derive(Debug)]
/// `LenIsGreaterThanCapacity` is a custom error type that is returned by [`Buffer::set_len`]
/// if the provided len is greater than the buffer capacity.
pub struct LenIsGreaterThanCapacity;

impl fmt::Display for LenIsGreaterThanCapacity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Provided len is greater than buffer capacity.")
    }
}

impl std::error::Error for LenIsGreaterThanCapacity {}

/// Buffer for data transfer. Buffer is allocated in heap.
///
/// Buffer has `len` and `cap`.
///
/// - `len` is how many bytes have been written into the buffer.
/// - `cap` is how many bytes have been allocated for the buffer.
///
/// # Why it is !Send
///
/// [`Buffer`] is not `Send`, because it can be __fixed__.
///
/// # About pool
///
/// For get from [`BufPool`], call [`buffer()`]
/// or [`full_buffer()`](crate::buf::full_buffer).
/// If you can use [`BufPool`], use it, to have better performance.
///
/// If it was gotten from [`BufPool`], it will come back after drop.
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
    #[cfg(not(target_os = "linux"))]
    os_buffer: ManuallyDrop<Vec<u8>>,
    #[cfg(target_os = "linux")]
    os_buffer: ManuallyDrop<LinuxBuffer>,
    // impl !Send
    no_send_marker: PhantomData<*const ()>,
}

impl Buffer {
    /// Creates new buffer with given size. This buffer will not be put to the pool.
    /// So, use it only for creating a buffer with specific size.
    ///
    /// # Safety
    ///
    /// - size > 0
    #[inline(always)]
    pub(crate) fn new(size: u32) -> Self {
        Self {
            #[cfg(not(target_os = "linux"))]
            os_buffer: ManuallyDrop::new(Vec::with_capacity(size as _)),
            #[cfg(target_os = "linux")]
            os_buffer: ManuallyDrop::new(LinuxBuffer::new_non_fixed(size)),
            no_send_marker: PhantomData,
        }
    }

    /// Creates new fixed buffer with given buffer.
    #[cfg(target_os = "linux")]
    #[inline(always)]
    pub(crate) fn new_fixed(ptr: *mut u8, cap: u32, index: u16) -> Self {
        Self {
            #[cfg(target_os = "linux")]
            os_buffer: ManuallyDrop::new(LinuxBuffer::new_fixed(ptr, cap, index)),
            no_send_marker: PhantomData,
        }
    }

    /// Creates a new buffer from a pool with the given size.
    #[inline(always)]
    pub(crate) fn new_from_pool(pool: &BufPool) -> Self {
        Self::new(pool.default_buffer_capacity())
    }

    /// Returns `true` if the buffer is empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len_u32() == 0
    }

    /// Sets [`len`](#field.len).
    ///
    /// # Safety
    ///
    /// `len` must be less than or equal to `capacity`.
    #[inline(always)]
    pub unsafe fn set_len_unchecked(&mut self, len: u32) {
        #[cfg(not(target_os = "linux"))]
        unsafe {
            self.os_buffer.set_len(len as usize);
        }

        #[cfg(target_os = "linux")]
        unsafe {
            self.os_buffer.set_len(len);
        }
    }

    /// Sets [`len`](#field.len). Returns `Err<()>` if `len` is greater than `capacity`.
    #[inline(always)]
    pub fn set_len(&mut self, len: u32) -> Result<(), LenIsGreaterThanCapacity> {
        if len <= self.capacity() {
            unsafe { self.set_len_unchecked(len) };

            Ok(())
        } else {
            Err(LenIsGreaterThanCapacity)
        }
    }

    /// Increases [`len`](#field.len) by the diff.
    ///
    /// # Safety
    ///
    /// `len` + `diff` must be less than or equal to `capacity`.
    #[inline(always)]
    pub unsafe fn add_len(&mut self, diff: u32) {
        let new_len = self.len_u32() + diff;

        unsafe {
            self.set_len_unchecked(new_len);
        }
    }

    /// Returns a real capacity of the buffer.
    #[inline(always)]
    pub fn capacity(&self) -> u32 {
        self.os_buffer.capacity()
    }

    /// Sets [`len`](#field.len) to [`real_cap`](#method.real_cap).
    #[inline(always)]
    pub fn set_len_to_capacity(&mut self) {
        let cap = self.capacity();

        unsafe {
            self.os_buffer.set_len(cap);
        }
    }

    /// Returns `true` if the buffer is full.
    #[inline(always)]
    pub fn is_full(&self) -> bool {
        self.capacity() == self.len_u32()
    }

    /// Resizes the buffer to a new size. If the buffer is __fixed__, after resizing it
    /// can become __not fixed__.
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
    /// ```no_run
    /// use orengine::Executor;
    /// use orengine::io::{buffer, Buffer};
    ///
    /// Executor::init().run_and_block_on_local(async {
    ///     let mut buf = buffer();
    ///
    ///     buf.append(&[1, 2, 3]);
    ///     buf.resize(20000); // Buffer is resized to 20000, contents are preserved
    ///
    ///     assert_eq!(buf.capacity(), 20000);
    ///     assert_eq!(buf.as_ref(), &[1, 2, 3]);
    /// }).unwrap();
    /// ```
    #[inline(always)]
    pub fn resize(&mut self, new_size: u32) {
        if new_size < self.len_u32() {
            unsafe {
                self.set_len_unchecked(new_size);
            }

            return;
        }

        let mut new_buf = if buf_pool().default_buffer_capacity() == new_size {
            buffer()
        } else {
            Self::new(new_size)
        };

        unsafe {
            new_buf.set_len_unchecked(self.len_u32());
            ptr::copy_nonoverlapping(self.as_ptr(), new_buf.as_mut_ptr(), self.len_u32() as usize);
        }

        *self = new_buf;
    }

    /// Appends data to the buffer.
    /// If a capacity is not enough, the buffer will be resized.
    #[inline(always)]
    pub fn append(&mut self, buf: &[u8]) {
        let diff_len = buf.len() as u32;
        if diff_len > self.capacity() - self.len_u32() {
            self.resize(self.len_u32() + diff_len);
        }

        unsafe {
            ptr::copy_nonoverlapping(
                buf.as_ptr(),
                self.as_mut_ptr().add(self.len_u32() as usize),
                buf.len(),
            );
            self.add_len(diff_len);
        }
    }

    /// Clears the buffer.
    #[inline(always)]
    pub fn clear(&mut self) {
        unsafe {
            self.set_len_unchecked(0);
        }
    }

    /// Fills provided `Buffer` with zeros.
    pub fn fill_with_zeros(&mut self) {
        unsafe {
            ptr::write_bytes(self.as_mut_ptr(), 0, self.capacity() as usize);
        }
    }

    /// Returns [`Slice`] with the specified range.
    #[inline(always)]
    pub fn slice<R: RangeBounds<u32>>(&self, range: R) -> Slice<'_> {
        let start = match range.start_bound() {
            Bound::Included(s) => *s,
            Bound::Unbounded => 0,
            Bound::Excluded(s) => *s + 1,
        };

        let end = match range.end_bound() {
            Bound::Included(e) => *e + 1,
            Bound::Excluded(e) => *e,
            Bound::Unbounded => self.len_u32(),
        };

        Slice::new(self, start, end)
    }

    /// Returns [`SliceMut`] with the specified range.
    #[inline(always)]
    pub fn slice_mut<R: RangeBounds<u32>>(&mut self, range: R) -> SliceMut<'_> {
        let start = match range.start_bound() {
            Bound::Included(s) => *s,
            Bound::Unbounded => 0,
            Bound::Excluded(s) => *s + 1,
        };

        let end = match range.end_bound() {
            Bound::Included(e) => *e + 1,
            Bound::Excluded(e) => *e,
            Bound::Unbounded => self.len_u32(),
        };

        SliceMut::new(self, start, end)
    }

    /// Puts the buffer to the pool. You can not to use it, and then this method will be called automatically by drop.
    #[inline(always)]
    pub fn release(self) {
        buf_pool().put(self);
    }

    /// Deallocates the buffer.
    #[inline(always)]
    pub(crate) fn deallocate(mut self) {
        #[cfg(not(target_os = "linux"))]
        {
            unsafe { ManuallyDrop::drop(&mut self.os_buffer) };
        }

        #[cfg(target_os = "linux")]
        unsafe {
            match ManuallyDrop::take(&mut self.os_buffer) {
                LinuxBuffer::Fixed(_) => {
                    // do nothing, because it is a reference to real buffer
                }
                LinuxBuffer::NonFixed(buf) => drop(buf),
            };
        }

        mem::forget(self);
    }
}

impl Sealed for Buffer {}

impl FixedBuffer for Buffer {
    #[inline(always)]
    fn as_ptr(&self) -> *const u8 {
        self.os_buffer.as_ptr().cast()
    }

    #[inline(always)]
    fn len_u32(&self) -> u32 {
        #[cfg(not(target_os = "linux"))]
        {
            return self.as_ref().len() as u32;
        }

        #[cfg(target_os = "linux")]
        {
            self.os_buffer.len()
        }
    }

    #[inline(always)]
    fn fixed_index(&self) -> u16 {
        #[cfg(not(target_os = "linux"))]
        {
            return u16::MAX;
        }

        #[cfg(target_os = "linux")]
        {
            match self.os_buffer.deref() {
                LinuxBuffer::Fixed(fixed_buf) => fixed_buf.index(),
                LinuxBuffer::NonFixed(_) => u16::MAX,
            }
        }
    }

    #[inline(always)]
    fn is_fixed(&self) -> bool {
        #[cfg(not(target_os = "linux"))]
        {
            return false;
        }

        #[cfg(target_os = "linux")]
        {
            match self.os_buffer.deref() {
                LinuxBuffer::Fixed(_) => true,
                LinuxBuffer::NonFixed(_) => false,
            }
        }
    }
}

impl FixedBufferMut for Buffer {
    #[inline(always)]
    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.os_buffer.as_mut_ptr().cast()
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
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        self.os_buffer.as_ref()
    }
}

impl AsMut<[u8]> for Buffer {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut [u8] {
        self.os_buffer.as_mut()
    }
}

impl PartialEq<&[u8]> for Buffer {
    #[inline(always)]
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
        #[cfg(not(target_os = "linux"))]
        return Self {
            os_buffer: ManuallyDrop::new(Vec::from(slice)),
            no_send_marker: PhantomData,
        };

        #[cfg(target_os = "linux")]
        Self {
            os_buffer: ManuallyDrop::new(LinuxBuffer::NonFixed(Vec::from(slice))),
            no_send_marker: PhantomData,
        }
    }
}

impl<const N: usize> From<Box<[u8; N]>> for Buffer {
    fn from(slice: Box<[u8; N]>) -> Self {
        #[cfg(not(target_os = "linux"))]
        return Self {
            os_buffer: ManuallyDrop::new(unsafe {
                Vec::from_raw_parts(Box::leak(slice).as_ptr().cast_mut(), N, N)
            }),
            no_send_marker: PhantomData,
        };

        #[cfg(target_os = "linux")]
        Self {
            os_buffer: ManuallyDrop::new(LinuxBuffer::NonFixed(unsafe {
                Vec::from_raw_parts(Box::leak(slice).as_ptr().cast_mut(), N, N)
            })),
            no_send_marker: PhantomData,
        }
    }
}

impl From<Vec<u8>> for Buffer {
    fn from(vec: Vec<u8>) -> Self {
        #[cfg(not(target_os = "linux"))]
        return Self {
            os_buffer: ManuallyDrop::new(vec),
            no_send_marker: PhantomData,
        };

        #[cfg(target_os = "linux")]
        Self {
            os_buffer: ManuallyDrop::new(LinuxBuffer::NonFixed(vec)),
            no_send_marker: PhantomData,
        }
    }
}

impl Drop for Buffer {
    #[inline(always)]
    fn drop(&mut self) {
        let buf_pool = buf_pool();
        unsafe { buf_pool.put(ptr::read(self)) };
    }
}

/// ```compile_fail
/// use orengine::{yield_now, Executor, Local};
/// use orengine::io::{full_buffer, AsyncWrite};
/// use orengine::fs::{File, OpenOptions};
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// Executor::init().run_with_shared_future(async {
///     let mut file = File::open("foo.txt", &OpenOptions::new().write(true)).await.unwrap();
///     let mut buf = full_buffer();
///     buf.append(b"hello");
///     file.write(&buf).await.unwrap();
///     drop(buf);
///
///     yield_now().await;
/// });
/// ```
///
/// ```no_run
/// use orengine::{yield_now, Executor, Local};
/// use orengine::io::{with_full_buffer, AsyncWrite};
/// use orengine::fs::{File, OpenOptions};
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// Executor::init().run_with_shared_future(async {
///     let mut file = File::open("./test/foo.txt", &OpenOptions::new().write(true).create(true)).await.unwrap();
///     with_full_buffer(|mut buf| async move {
///         buf.append(b"hello");
///         file.write(&buf).await.unwrap();
///     }).await;
///
///     yield_now().await;
/// });
/// ```
#[allow(dead_code, reason = "It is used only in compile tests")]
fn test_compile_buffer() {}
