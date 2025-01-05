use crate::io::worker::local_worker;
use crate::io::{Buffer, FixedBuffer};
use crate::utils::assert_hint;
use libc;
use std::cell::UnsafeCell;
use std::io::IoSliceMut;

thread_local! {
    /// Local [`BufPool`]. Therefore, it is lockless.
    static BUF_POOL: UnsafeCell<Option<BufPool>> = const {
        UnsafeCell::new(None)
    };
}

/// Initialize local [`BufPool`] and register __fixed__ buffers.
pub(crate) fn init_local_buf_pool(number_of_fixed_buffers: u16, default_buffer_cap: u32) {
    BUF_POOL.with(|buf_pool_static| {
        let buf_pool_ = unsafe { &mut *buf_pool_static.get() };
        assert!(buf_pool_.is_none(), "BufPool is already initialized.");

        let buf_pool = BufPool::new(number_of_fixed_buffers, default_buffer_cap);
        *buf_pool_ = Some(buf_pool);
    });
}

/// Uninitialize local [`BufPool`].
pub(crate) fn uninit_local_buf_pool() {
    BUF_POOL.with(|buf_pool_static| unsafe {
        let buf_pool_ = &mut *buf_pool_static.get();
        assert!(buf_pool_.is_some(), "BufPool is not initialized.");

        *buf_pool_ = None;
    });
}

/// Get [`BufPool`] from thread local. Therefore, it is lockless.
///
/// # Panics
///
/// If [`BufPool`] is not initialized.
///
/// # Undefined behavior
///
/// If [`BufPool`] is not initialized in __release__ build.
#[inline(always)]
pub fn buf_pool() -> &'static mut BufPool {
    BUF_POOL.with(|buf_pool| unsafe {
        let buf_pool_ = &mut *buf_pool.get();
        assert_hint(buf_pool_.is_some(), "BufPool is not initialized.");

        buf_pool_.as_mut().unwrap()
    })
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
/// use orengine::io::full_buffer;
/// use orengine::io::{AsyncPollSocket, AsyncRecv};
/// use orengine::net::TcpStream;
///
/// async fn handle_connection(mut stream: TcpStream) {
///     stream.poll_recv().await.expect("Failed to poll stream");
///     let mut buf = full_buffer();
///     let n = stream.recv(&mut buf).await.expect("Failed to read") as usize;
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

/// Pool of [`Buffer`]s. It is used for reusing memory.
pub struct BufPool {
    #[cfg(target_os = "linux")]
    pub(crate) pool_of_fixed_buffers: Vec<Buffer>,
    #[cfg(not(target_os = "linux"))]
    pub(crate) pool: Vec<Buffer>,
    #[cfg(target_os = "linux")]
    pub(crate) fixed_buffers: Box<[IoSliceMut<'static>]>,
    default_buffer_cap: u32,
    #[cfg(target_os = "linux")]
    pub(crate) pool_of_non_fixed_buffers: Vec<Buffer>,
}

impl BufPool {
    /// Creates new [`BufPool`]. If `number_of_fixed_buffers` > 0,
    /// it creates __fixed__ buffers.
    fn new(number_of_fixed_buffers: u16, default_buffer_cap: u32) -> Self {
        #[cfg(not(target_os = "linux"))]
        {
            Self {
                pool: (0..number_of_fixed_buffers)
                    .map(|_| Buffer::new(default_buffer_cap))
                    .collect(),
                default_buffer_cap,
            }
        }

        #[cfg(target_os = "linux")]
        {
            if number_of_fixed_buffers == 0 {
                return Self {
                    fixed_buffers: Box::new([]),
                    pool_of_fixed_buffers: Vec::new(),
                    default_buffer_cap,
                    pool_of_non_fixed_buffers: Vec::new(),
                };
            }

            let mut fixed_buffers: Box<[IoSliceMut<'static>]> = (0..number_of_fixed_buffers)
                .map(|_| {
                    let slice = Box::leak(vec![0; default_buffer_cap as _].into_boxed_slice());
                    IoSliceMut::new(slice)
                })
                .collect();
            let pool_of_fixed_buffers = fixed_buffers
                .iter_mut()
                .enumerate()
                .map(|(i, buf)| {
                    Buffer::new_fixed(
                        buf.as_mut_ptr(),
                        default_buffer_cap as _,
                        u16::try_from(i).expect("number_of_fixed_buffers > u16::MAX"),
                    )
                })
                .collect();
            let iovecs: Vec<libc::iovec> = fixed_buffers
                .iter_mut()
                .map(|buf| libc::iovec {
                    iov_base: buf.as_mut_ptr().cast(),
                    iov_len: buf.len() as _,
                })
                .collect();
            local_worker().register_buffers(&iovecs);

            Self {
                fixed_buffers,
                pool_of_fixed_buffers,
                default_buffer_cap,
                pool_of_non_fixed_buffers: Vec::new(),
            }
        }
    }

    /// Deallocates the `BufPool` and deregisters __fixed__ buffers.
    fn deallocate_buffers(&mut self) {
        #[cfg(not(target_os = "linux"))]
        {
            for buf in self.pool.drain(..) {
                buf.deallocate();
            }
        }

        #[cfg(target_os = "linux")]
        {
            for buf in self.pool_of_non_fixed_buffers.drain(..) {
                buf.deallocate();
            }

            if self.pool_of_fixed_buffers.is_empty() {
                return;
            }

            for buf in self.pool_of_fixed_buffers.drain(..) {
                buf.deallocate();
            }

            local_worker().deregister_buffers();

            for buf in &mut self.fixed_buffers {
                unsafe { drop(Box::from_raw(std::ptr::from_mut::<[u8]>(buf.as_mut()))) };
            }
        }
    }

    /// Returns the number of buffers in the pool. Uses in tests.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        #[cfg(not(target_os = "linux"))]
        {
            self.pool.len()
        }

        #[cfg(target_os = "linux")]
        {
            self.pool_of_fixed_buffers.len() + self.pool_of_non_fixed_buffers.len()
        }
    }

    /// Returns default buffer capacity.
    #[inline(always)]
    pub fn default_buffer_capacity(&self) -> u32 {
        self.default_buffer_cap
    }

    /// Returns [`Buffer`] from the pool. It returns __fixed__ buffer if it is possible.
    ///
    /// This method doesn't guarantee any len of the buffer.
    /// Returned buffer is filled with any value (not only 0).
    #[inline(always)]
    pub fn get_buffer_with_any_len(&mut self) -> Buffer {
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

    /// Gets [`Buffer`] from [`BufPool`] with full length. It returns __fixed__ buffer
    /// if it is possible.
    ///
    /// Returned buffer is filled with any value (not only 0).
    #[inline(always)]
    pub fn get_full(&mut self) -> Buffer {
        let mut buffer = self.get_buffer_with_any_len();
        buffer.set_len_to_capacity();

        buffer
    }

    /// Gets empty (len == 0) [`Buffer`] from [`BufPool`]. It returns __fixed__ buffer
    /// if it is possible.
    ///
    /// Returned buffer is filled with any value (not only 0).
    #[inline(always)]
    pub fn get(&mut self) -> Buffer {
        let mut buffer = self.get_buffer_with_any_len();
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

impl Drop for BufPool {
    fn drop(&mut self) {
        self.deallocate_buffers();
    }
}

/// Returns __fixed__ buffer from the pool. It used only in tests.
#[allow(clippy::future_not_send, reason = "It is a test.")]
#[cfg(test)]
pub(crate) async fn get_fixed_buffer() -> Buffer {
    if cfg!(target_os = "linux") {
        let mut buf = buffer();

        while !buf.is_fixed() {
            crate::yield_now().await;
            buf = buffer();
        }

        buf
    } else {
        panic!("get_fixed is not supported on this OS.");
    }
}

/// Returns full (len == capacity) __fixed__ buffer from the pool. It used only in tests.
#[allow(clippy::future_not_send, reason = "It is a test.")]
#[cfg(test)]
pub(crate) async fn get_full_fixed_buffer() -> Buffer {
    if cfg!(target_os = "linux") {
        let mut buf = full_buffer();

        while !buf.is_fixed() {
            crate::yield_now().await;
            buf = buffer();
        }

        buf
    } else {
        panic!("get_fixed is not supported on this OS.");
    }
}
