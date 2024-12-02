use crate::buf::buf_pool::{buf_pool, buffer, BufPool};
use crate::buf::linux::linux_buffer::LinuxBuffer;
use std::fmt::Debug;
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::ptr;
use std::slice::SliceIndex;

/// Buffer for data transfer. Buffer is allocated in heap.
///
/// Buffer has `len` and `cap`.
///
/// - `len` is how many bytes have been written into the buffer.
/// - `cap` is how many bytes have been allocated for the buffer.
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
}

impl Buffer {
    /// Creates new buffer with given size. This buffer will not be put to the pool.
    /// So, use it only for creating a buffer with specific size.
    ///
    /// # Safety
    ///
    /// - size > 0
    #[inline(always)]
    pub fn new(size: usize) -> Self {
        Self {
            #[cfg(not(target_os = "linux"))]
            os_buffer: ManuallyDrop::new(Vec::with_capacity(size)),
            #[cfg(target_os = "linux")]
            os_buffer: ManuallyDrop::new(LinuxBuffer::new_non_fixed(size)),
        }
    }

    /// Creates a new buffer from a pool with the given size.
    #[inline(always)]
    pub(crate) fn new_from_pool(pool: &BufPool) -> Self {
        Self::new(pool.default_buffer_cap())
    }

    /// Returns how many bytes have been written into the buffer.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.os_buffer.len()
    }

    /// Returns `true` if the buffer is empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Sets [`len`](#field.len).
    ///
    /// # Safety
    ///
    /// `len` must be less than or equal to `capacity`.
    #[inline(always)]
    pub unsafe fn set_len(&mut self, len: usize) {
        unsafe {
            self.os_buffer.set_len(len);
        }
    }

    /// Increases [`len`](#field.len) by the diff.
    ///
    /// # Safety
    ///
    /// `len` + `diff` must be less than or equal to `capacity`.
    #[inline(always)]
    pub unsafe fn add_len(&mut self, diff: usize) {
        let new_len = self.len() + diff;

        unsafe {
            self.os_buffer.set_len(new_len);
        }
    }

    /// Returns a real capacity of the buffer.
    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.os_buffer.capacity()
    }

    /// Sets [`len`](#field.len) to [`real_cap`](#method.real_cap).
    #[inline(always)]
    pub fn set_len_to_cap(&mut self) {
        let cap = self.capacity();

        unsafe {
            self.os_buffer.set_len(cap);
        }
    }

    /// Returns `true` if the buffer is full.
    #[inline(always)]
    pub fn is_full(&self) -> bool {
        self.capacity() == self.len()
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
    /// assert_eq!(buf.capacity(), 200);
    /// assert_eq!(buf.as_ref(), &[1, 2, 3]);
    /// ```
    #[inline(always)]
    pub fn resize(&mut self, new_size: usize) {
        if new_size < self.len() {
            unsafe {
                self.set_len(new_size);
            }

            return;
        }

        let mut new_buf = if buf_pool().default_buffer_cap() == new_size {
            buffer()
        } else {
            Self::new(new_size)
        };

        unsafe {
            new_buf.set_len(self.len());
            ptr::copy_nonoverlapping(self.as_ptr(), new_buf.as_mut_ptr(), self.len());
        }

        *self = new_buf;
    }

    /// Appends data to the buffer.
    /// If a capacity is not enough, the buffer will be resized.
    #[inline(always)]
    pub fn append(&mut self, buf: &[u8]) {
        let diff_len = buf.len();
        if diff_len > self.capacity() - self.len() {
            self.resize(self.len() + diff_len);
        }

        unsafe {
            ptr::copy_nonoverlapping(buf.as_ptr(), self.as_mut_ptr().add(self.len()), diff_len);
            self.add_len(diff_len);
        }
    }

    /// Returns a pointer to the buffer.
    #[inline(always)]
    pub fn as_ptr(&self) -> *const u8 {
        self.os_buffer.as_ptr().cast()
    }

    /// Returns a mutable pointer to the buffer.
    #[inline(always)]
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.os_buffer.as_mut_ptr().cast()
    }

    /// Clears the buffer.
    #[inline(always)]
    pub fn clear(&mut self) {
        unsafe {
            self.set_len(0);
        }
    }

    /// Puts the buffer to the pool. You can not to use it, and then this method will be called automatically by drop.
    #[inline(always)]
    pub fn release(self) {
        buf_pool().put(self);
    }

    /// Puts the buffer to the pool without checking for a size.
    ///
    /// # Safety
    ///
    /// - [`buf.real_cap`](#method.real_cap) is equal to
    ///   [`default cap`](BufPool::default_buffer_cap)
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
        };

        #[cfg(target_os = "linux")]
        Self {
            os_buffer: ManuallyDrop::new(LinuxBuffer::NonFixed(Vec::from(slice))),
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
        };

        #[cfg(target_os = "linux")]
        Self {
            os_buffer: ManuallyDrop::new(LinuxBuffer::NonFixed(unsafe {
                Vec::from_raw_parts(Box::leak(slice).as_ptr().cast_mut(), N, N)
            })),
        }
    }
}

impl From<Vec<u8>> for Buffer {
    fn from(vec: Vec<u8>) -> Self {
        #[cfg(not(target_os = "linux"))]
        return Self {
            os_buffer: ManuallyDrop::new(vec),
        };

        #[cfg(target_os = "linux")]
        Self {
            os_buffer: ManuallyDrop::new(LinuxBuffer::NonFixed(vec)),
        }
    }
}

unsafe impl Send for Buffer {}

impl Drop for Buffer {
    #[inline(always)]
    fn drop(&mut self) {
        let buf_pool = buf_pool();
        #[cfg(not(target_os = "linux"))]
        {
            if self.capacity() == buf_pool.default_buffer_cap() {
                unsafe { buf_pool.put_unchecked(ptr::read(self)) };
            } else {
                unsafe { ManuallyDrop::drop(&mut self.os_buffer) };
            }
        }

        #[cfg(target_os = "linux")]
        {
            match self.os_buffer {
                LinuxBuffer::Fixed(_) => {
                    unsafe { buf_pool.put_unchecked(ptr::read(self)) };
                }
                LinuxBuffer::NonFixed(_) => {
                    if self.capacity() == buf_pool.default_buffer_cap() {
                        unsafe { buf_pool.put_unchecked(ptr::read(self)) };
                    } else {
                        unsafe { ManuallyDrop::drop(&mut self.os_buffer) };
                    }
                }}
            }
        }
    }
}
