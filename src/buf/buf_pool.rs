use crate::buf::Buffer;
use crate::local_executor;
use crate::runtime::config::DEFAULT_BUF_CAP;
use std::cell::UnsafeCell;
use std::mem::ManuallyDrop;

thread_local! {
    /// Local [`BufPool`]. Therefore, it is lockless.
    pub(crate) static BUF_POOL: UnsafeCell<ManuallyDrop<BufPool>> = UnsafeCell::new(
        ManuallyDrop::new(BufPool::new())
    );
}

/// Get [`BufPool`] from thread local. Therefore, it is lockless.
#[inline(always)]
pub fn buf_pool() -> &'static mut BufPool {
    BUF_POOL.with(|buf_pool| unsafe { &mut *buf_pool.get() })
}

/// Get [`Buffer`] from local [`BufPool`].
///
/// Please, don't keep the buffer longer than necessary.
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
    buffer_len: usize,
}

impl BufPool {
    const fn new() -> Self {
        Self {
            pool: Vec::new(),
            buffer_len: DEFAULT_BUF_CAP,
        }
    }

    /// Get default buffer size.
    pub fn buffer_len(&self) -> usize {
        self.buffer_len
    }

    /// Change default buffer capacity.
    pub fn tune_buffer_cap(&mut self, buffer_cap: usize) {
        if self.buffer_len == buffer_cap {
            return;
        }
        local_executor().set_config_buffer_cap(self.buffer_len);
        self.buffer_len = buffer_cap;
        self.pool = Vec::new();
    }

    /// Get [`Buffer`] from [`BufPool`].
    pub fn get(&mut self) -> Buffer {
        match self.pool.pop() {
            Some(buf) => buf,
            None => Buffer::new_from_pool(self.buffer_len),
        }
    }

    /// Put [`Buffer`] to [`BufPool`].
    pub fn put(&mut self, buf: Buffer) {
        if buf.cap() == self.buffer_len {
            unsafe {
                self.put_unchecked(buf);
            }
        }
    }

    /// Put [`Buffer`] to [`BufPool`] without checking for a size.
    ///
    /// # Safety
    ///
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
    use crate as orengine;

    #[orengine_macros::test_local]
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
