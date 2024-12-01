use crate::buf::Buffer;
use crate::local_executor;
use crate::runtime::config::DEFAULT_BUF_CAP;
use std::cell::UnsafeCell;
use std::mem::ManuallyDrop;

thread_local! {
    /// Local [`BufPool`]. Therefore, it is lockless.
    pub(crate) static BUF_POOL: UnsafeCell<ManuallyDrop<BufPool>> = const {
        UnsafeCell::new(
            ManuallyDrop::new(BufPool::new())
        )
    };
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
/// ```rust
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
    buf_pool().get_full()
}

/// Pool of [`Buffer`]s. It is used for reusing memory. If you need to change default buffer size,
/// use [`BufPool::tune_buffer_cap`].
pub struct BufPool {
    pool: Vec<Buffer>,
    default_buffer_cap: usize,
}

impl BufPool {
    const fn new() -> Self {
        Self {
            pool: Vec::new(),
            default_buffer_cap: DEFAULT_BUF_CAP,
        }
    }

    /// Get default buffer capacity. It can be set with [`BufPool::tune_buffer_cap`].
    #[inline(always)]
    pub fn default_buffer_cap(&self) -> usize {
        self.default_buffer_cap
    }

    /// Change default buffer capacity.
    pub fn tune_buffer_cap(&mut self, buffer_cap: usize) {
        if self.default_buffer_cap == buffer_cap {
            return;
        }
        local_executor().set_config_buffer_cap(self.default_buffer_cap);
        self.default_buffer_cap = buffer_cap;
        self.pool = Vec::new();
    }

    /// Get [`Buffer`] from [`BufPool`] with full length.
    #[inline(always)]
    pub fn get_full(&mut self) -> Buffer {
        let mut pool = self
            .pool
            .pop()
            .map_or_else(|| Buffer::new_from_pool(self), |buf| buf);
        pool.set_len_to_cap();

        pool
    }

    /// Get [`Buffer`] from [`BufPool`].
    #[inline(always)]
    pub fn get(&mut self) -> Buffer {
        let mut pool = self
            .pool
            .pop()
            .map_or_else(|| Buffer::new_from_pool(self), |buf| buf);
        pool.clear();

        pool
    }

    /// Put [`Buffer`] to [`BufPool`].
    #[inline(always)]
    pub fn put(&mut self, buf: Buffer) {
        if buf.cap() == self.default_buffer_cap {
            unsafe {
                self.put_unchecked(buf);
            }
        }
    }

    /// Put [`Buffer`] to [`BufPool`] without checking for a size.
    ///
    /// # Safety
    ///
    /// - `buf.cap()` == `self.buffer_len`
    #[inline(always)]
    pub unsafe fn put_unchecked(&mut self, buf: Buffer) {
        self.pool.push(buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;

    #[orengine::test::test_local]
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
