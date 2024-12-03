use crate::buf::io_buffer::IOBuffer;
use crate::buf::Buffer;
use std::cell::UnsafeCell;
use std::io::IoSliceMut;
use std::mem::ManuallyDrop;

thread_local! {
    /// Local [`BufPool`]. Therefore, it is lockless.
    static BUF_POOL: UnsafeCell<ManuallyDrop<BufPool>> = const {
        UnsafeCell::new(
            ManuallyDrop::new(BufPool{
                default_buffer_cap: usize::MAX // if BufPool is not initialized,
                // it will panic because of trying to allocate usize::MAX bytes.
                #[cfg(not(target_os = "linux"))]
                pool: Vec::new(),
                #[cfg(target_os = "linux")]
                pool_of_fixed_buffers: Vec::new(),
                #[cfg(target_os = "linux")]
                pool_of_non_fixed_buffers: Vec::new(),
                #[cfg(target_os = "linux")]
                fixed_buffers: Box::new([]),
            })
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
/// use orengine::buf::full_buffer;
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
    #[cfg(target_os = "linux")]
    pub(crate) pool_of_fixed_buffers: Vec<Buffer>,
    #[cfg(not(target_os = "linux"))]
    pub(crate) pool: Vec<Buffer>,
    #[cfg(target_os = "linux")]
    pub(crate) fixed_buffers: Box<[IoSliceMut<'static>]>,
    default_buffer_cap: usize,
    #[cfg(target_os = "linux")]
    pub(crate) pool_of_non_fixed_buffers: Vec<Buffer>,
}

impl BufPool {
    fn new(number_of_preallocated_buffers: usize, default_buffer_cap: usize) -> Self {
        #[cfg(not(target_os = "linux"))]
        {
            Self {
                pool: (0..number_of_preallocated_buffers)
                    .map(|_| Buffer::new(default_buffer_cap))
                    .collect(),
                default_buffer_cap,
            }
        }

        #[cfg(target_os = "linux")]
        {
            let mut slices: Box<[IoSliceMut<'static>]> = (0..number_of_preallocated_buffers)
                .map(|_| {
                    let slice = Box::leak(vec![0; default_buffer_cap].into_boxed_slice());
                    IoSliceMut::new(slice)
                })
                .collect();
            let fixed_buffers = slices
                .iter_mut()
                .enumerate()
                .map(|(i, buf)| Buffer::new_fixed(buf.as_mut_ptr(), default_buffer_cap, i))
                .collect();

            Self {
                fixed_buffers: slices,
                pool_of_fixed_buffers: Vec::new(),
                default_buffer_cap,
                pool_of_non_fixed_buffers: fixed_buffers,
            }
        }
    }

    /// Get default buffer capacity. It can be set with [`BufPool::tune_buffer_cap`].
    #[inline(always)]
    pub fn default_buffer_capacity(&self) -> usize {
        self.default_buffer_cap
    }

    /// Returns [`Buffer`] from the pool. It will return __fixed__ buffer if it is possible.
    ///
    /// This method doesn't guarantee any len of the buffer.
    #[inline(always)]
    fn get_any_buffer(&mut self) -> Buffer {
        #[cfg(not(target_os = "linux"))]
        {
            self.pool
                .pop()
                .unwrap_or_else(|| Buffer::new_from_pool(self))
        }

        #[cfg(target_os = "linux")]
        if let Some(fixed_buf) = self.pool_of_fixed_buffers.pop() {
            fixed_buf
        } else {
            self.pool_of_non_fixed_buffers
                .pop()
                .unwrap_or_else(|| Buffer::new_from_pool(self))
        }
    }

    /// Gets [`Buffer`] from [`BufPool`] with full length.
    #[inline(always)]
    pub fn get_full(&mut self) -> Buffer {
        let mut buffer = self.get_any_buffer();
        buffer.set_len_to_capacity();

        buffer
    }

    /// Gets empty [`Buffer`] from [`BufPool`].
    #[inline(always)]
    pub fn get(&mut self) -> Buffer {
        let mut buffer = self.get_any_buffer();
        buffer.clear();

        buffer
    }

    /// Tries to put [`Buffer`] to [`BufPool`].
    ///
    /// If provided [`Buffer`] has no the same capacity as
    /// [`default buffer capacity`](Self::default_buffer_capacity), it will be drooped.
    #[inline(always)]
    pub(crate) fn put(&mut self, buf: Buffer) {
        #[cfg(not(target_os = "linux"))]
        {
            if buf.capacity() == self.default_buffer_cap {
                unsafe {
                    self.pool.push(buf);
                }
            }

            buf.deallocate();

            return;
        }

        #[cfg(target_os = "linux")]
        {
            if buf.is_fixed() {
                self.pool_of_fixed_buffers.push(buf);
            } else if buf.capacity() == self.default_buffer_cap {
                self.pool_of_non_fixed_buffers.push(buf);
            } else {
                buf.deallocate();
            }
        }
    }
}
