use crate::buf::buf_pool::{buf_pool, buffer, BufPool};
use crate::buf::io_buffer::{IOBuffer, IOBufferMut};
use crate::buf::linux::linux_buffer::LinuxBuffer;
use std::fmt::Debug;
use std::marker::PhantomData;
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
    pub fn new(size: u32) -> Self {
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
        self.len() == 0
    }

    /// Sets [`len`](#field.len).
    ///
    /// # Safety
    ///
    /// `len` must be less than or equal to `capacity`.
    #[inline(always)]
    pub unsafe fn set_len(&mut self, len: u32) {
        #[cfg(not(target_os = "linux"))]
        unsafe {
            self.os_buffer.set_len(len as usize);
        }

        #[cfg(target_os = "linux")]
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
    pub unsafe fn add_len(&mut self, diff: u32) {
        let new_len = self.len() + diff;

        unsafe {
            self.set_len(new_len);
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
    pub fn resize(&mut self, new_size: u32) {
        if new_size < self.len() {
            unsafe {
                self.set_len(new_size);
            }

            return;
        }

        let mut new_buf = if buf_pool().default_buffer_capacity() == new_size {
            buffer()
        } else {
            Self::new(new_size)
        };

        unsafe {
            new_buf.set_len(self.len());
            ptr::copy_nonoverlapping(self.as_ptr(), new_buf.as_mut_ptr(), self.len() as usize);
        }

        *self = new_buf;
    }

    /// Appends data to the buffer.
    /// If a capacity is not enough, the buffer will be resized.
    #[inline(always)]
    pub fn append(&mut self, buf: &[u8]) {
        let diff_len = buf.len() as u32;
        if diff_len > self.capacity() - self.len() {
            self.resize(self.len() + diff_len);
        }

        unsafe {
            ptr::copy_nonoverlapping(
                buf.as_ptr(),
                self.as_mut_ptr().add(self.len() as usize),
                buf.len(),
            );
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

    /// Fills provided `Buffer` with zeros.
    pub fn fill_with_zeros(&mut self) {
        unsafe {
            ptr::write_bytes(self.as_mut_ptr(), 0, self.capacity() as usize);
        }
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
    }
}

impl IOBuffer for Buffer {
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
    fn len(&self) -> u32 {
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

impl IOBufferMut for Buffer {}

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

/// ```rust
/// use orengine::{Executor, Local};
/// use orengine::buf::full_buffer;
/// use orengine::fs::{File, OpenOptions};
/// use orengine::io::AsyncWrite;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// Executor::init().run_with_shared_future(async {
///     let mut buf = full_buffer();
///     buf.append(b"hello");
///     let mut file = File::open("foo.txt", &OpenOptions::new().write(true)).await.unwrap();
///     file.write(buf);
/// });
/// ```
///
/// ```rust
/// use orengine::Local;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// let value = Local::new(42);
/// check_send(value.borrow());
/// ```
///
/// ```rust
/// use orengine::Local;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// let value = Local::new(42);
/// check_send(value.borrow_mut());
/// ```
#[allow(dead_code, reason = "It is used only in compile tests")]
fn test_compile_local() {}
