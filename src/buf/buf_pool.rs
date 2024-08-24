use std::cell::UnsafeCell;
use std::intrinsics::{likely, unlikely};
use std::mem::MaybeUninit;
use crate::buf::Buffer;
use crate::cfg::{set_buf_len};

thread_local! {
    /// Local [`BufPool`]. So, it is lockless.
    pub static BUF_POOL: UnsafeCell<MaybeUninit<BufPool>> = UnsafeCell::new(MaybeUninit::uninit());
}

/// Get [`BufPool`] from thread local. So, it is lockless.
#[inline(always)]
pub fn buf_pool() -> &'static mut BufPool {
    BUF_POOL.with(|pool| {
        unsafe { (&mut *pool.get()).assume_init_mut() }
    })
}

/// Get [`Buffer`] from local [`BufPool`].
///
/// Please, do not keep the buffer longer than necessary.
/// After drop, it will be returned to the pool.
#[inline(always)]
pub fn buffer() -> Buffer {
    buf_pool().get()
}

/// Get [`Buffer`] from local [`BufPool`] and set its length to its capacity.
///
/// # Usage
///
/// Use [`full_buffer`] if you need to read into the buffer,
/// because [`buffer`] returns empty buffer.
///
/// ```no_run
/// use orengine::buf::buf_pool::full_buffer;
/// use orengine::io::{AsyncPollFd, AsyncRecv};
/// use orengine::net::TcpStream;
///
/// async fn handle_connection(mut stream: TcpStream) {
///     stream.poll_recv().await.expect("Failed to poll stream");
///     let mut buf = full_buffer();
///     let n = stream.recv(&mut buf).await.expect("Failed to read");
///     println!("Received message: {}", String::from_utf8_lossy(&buf[..n]));
/// }
/// ```
///
/// # Attention
///
/// [`full_buffer`] returns full buffer, but it is filled with any value (not only 0).
///
/// Please, do not keep the buffer longer than necessary.
/// After drop, it will be returned to the pool.
#[inline(always)]
pub fn full_buffer() -> Buffer {
    let mut buf = buf_pool().get();
    buf.set_len_to_cap();
    buf
}

/// Pool of [`Buffer`]s. It is used for reusing memory. If you need to change default buffer size,
/// use [`BufPool::tune_buffer_len`].
pub struct BufPool {
    pool: Vec<Buffer>,
    buffer_len: usize
}

impl BufPool {
    /// Initialize [`BufPool`] in local thread.
    pub fn init_in_local_thread(buffer_len: usize) {
        BUF_POOL.with(|pool| {
            let pool_ref = unsafe { &mut *pool.get() };
            *pool_ref = MaybeUninit::new(BufPool {
                pool: Vec::with_capacity(0),
                buffer_len
            });
        });
    }

    /// Uninitialize [`BufPool`] in local thread.
    pub(crate) fn uninit_in_local_thread() {
        BUF_POOL.with(|pool| {
            unsafe { (&mut *pool.get()).assume_init_drop()};
        });
    }

    /// Get default buffer size.
    pub fn buffer_len(&self) -> usize {
        self.buffer_len
    }

    /// Change default buffer size.
    pub fn tune_buffer_len(&mut self, buffer_len: usize) {
        if self.buffer_len == buffer_len {
            return;
        }
        set_buf_len(buffer_len);
        self.buffer_len = buffer_len;
        self.pool = Vec::with_capacity(0);
    }

    /// Get [`Buffer`] from [`BufPool`].
    pub fn get(&mut self) -> Buffer {
        if unlikely(self.pool.is_empty()) {
            return Buffer::new_from_pool(self.buffer_len);
        }

        unsafe { self.pool.pop().unwrap_unchecked() }
    }

    /// Put [`Buffer`] to [`BufPool`].
    pub fn put(&mut self, buf: Buffer) {
        if likely(buf.cap() == self.buffer_len) {
            unsafe { self.put_unchecked(buf); }
        }
    }

    /// Put [`Buffer`] to [`BufPool`] without checking for a size.
    ///
    /// # Safety
    /// - buf.cap() == self.buffer_len
    #[inline(always)]
    pub unsafe fn put_unchecked(&mut self, mut buf: Buffer) {
        buf.clear();
        self.pool.push(buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_macro::test]
    fn test_buf_pool() {
        let pool = buf_pool();
        assert!(pool.pool.is_empty());

        let buf = buffer();
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.cap(), 4096);
        drop(buf);

        let buf = full_buffer();
        assert_eq!(buf.len(), 4096);
        assert_eq!(buf.cap(), 4096);
        drop(buf);

        assert_eq!(pool.pool.len(), 1);

        let _buf = pool.get();
        assert!(pool.pool.is_empty());
    }
}