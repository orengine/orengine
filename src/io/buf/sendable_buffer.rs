use crate::io::{Buffer, FixedBuffer, FixedBufferMut, SendableSlice, SendableSliceMut};
use crate::utils::Sealed;
use std::collections::Bound;
use std::ops::{Deref, DerefMut, RangeBounds};

/// `SendableBuffer` is a wrapper struct that tells the compiler that the [`Buffer`] is
/// [`sendable`](Send). But only The caller  should ensure that it never sends to another thread.
///
/// ```no_run
/// use std::ops::Deref;
/// use orengine::Executor;
/// use orengine::fs::File;
/// use orengine::io::{buffer, AsyncWrite, SendableBuffer};
///
/// Executor::init().run_and_block_on_shared(async {
///     let mut file = File::open("./example.txt", &orengine::fs::OpenOptions::new().read(true))
///         .await
///         .unwrap();
///     let mut buffer = unsafe { SendableBuffer::from_buffer(buffer()) };
///     buffer.append("Hello, world!".as_bytes());
///     file.write_all(&mut buffer).await.unwrap();
/// }).unwrap();
/// ```
pub struct SendableBuffer {
    buf: Buffer,
}

impl SendableBuffer {
    /// Creates [`SendableBuffer`] from [`Buffer`].
    ///
    /// # Safety
    ///
    /// The caller must ensure that the [`Buffer`] never sends to another thread.
    pub unsafe fn from_buffer(buffer: Buffer) -> Self {
        Self { buf: buffer }
    }

    /// Returns [`SendableSlice`] with the specified range.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use orengine::Executor;
    /// use orengine::fs::{File, OpenOptions};
    /// use orengine::io::{buffer, AsyncRead, AsyncWrite, SendableBuffer};
    ///
    /// # Executor::init().run_and_block_on_shared(async {
    /// let mut file = File::open("./foo.txt", &OpenOptions::new().write(true).create(true)).await.unwrap();
    /// let mut buf = unsafe { SendableBuffer::from_buffer(buffer()) };
    ///
    /// file.read_exact(&mut buf.slice_mut(..100)).await.unwrap(); // read exactly 100 bytes
    /// # }).unwrap();
    pub fn slice<R: RangeBounds<u32>>(&self, range: R) -> SendableSlice {
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

        SendableSlice::new(self, start, end)
    }

    /// Returns [`SendableSlice`] with the specified range.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use orengine::Executor;
    /// use orengine::fs::{File, OpenOptions};
    /// use orengine::io::{full_buffer, AsyncWrite, SendableBuffer};
    ///
    /// # Executor::init().run_and_block_on_shared(async {
    /// let mut file = File::open("./foo.txt", &OpenOptions::new().read(true).write(true).create(true)).await.unwrap();
    /// let mut buf = unsafe { SendableBuffer::from_buffer(full_buffer()) };
    ///
    /// buf.append(&*vec![1u8; 200]);
    ///
    /// file.write_all(&mut buf.slice(..100)).await.unwrap(); // write exactly 100 bytes
    /// # }).unwrap();
    /// ```
    pub fn slice_mut<R: RangeBounds<u32>>(&mut self, range: R) -> SendableSliceMut {
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

        SendableSliceMut::new(self, start, end)
    }
}

#[allow(
    clippy::non_send_fields_in_send_ty,
    reason = "The caller  guarantees that `SendableBuffer` is `Send`."
)]
unsafe impl Send for SendableBuffer {}
unsafe impl Sync for SendableBuffer {}

impl Deref for SendableBuffer {
    type Target = Buffer;

    fn deref(&self) -> &Self::Target {
        &self.buf
    }
}

impl DerefMut for SendableBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buf
    }
}

impl Sealed for SendableBuffer {}

impl FixedBuffer for SendableBuffer {
    fn as_ptr(&self) -> *const u8 {
        self.buf.as_ptr()
    }

    fn len_u32(&self) -> u32 {
        self.buf.len_u32()
    }

    fn fixed_index(&self) -> u16 {
        self.buf.fixed_index()
    }

    fn is_fixed(&self) -> bool {
        self.buf.is_fixed()
    }
}

impl FixedBufferMut for SendableBuffer {
    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.buf.as_mut_ptr()
    }
}
