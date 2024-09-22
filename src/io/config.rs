use crate::io::sys::unix::IoUringConfig;

/// Config for [`IoWorker`](crate::io::worker::IoWorker).
#[derive(Clone, Copy)]
pub struct IoWorkerConfig {
    /// Config for [`IoUringWorker`](crate::io::sys::unix::IoUringWorker).
    ///
    /// Read [`IoUringConfig`](IoUringConfig) for more details.
    pub(crate) io_uring: IoUringConfig
}

impl IoWorkerConfig {
    /// Returns default [`IoWorkerConfig`]
    pub const fn default() -> Self {
        Self { io_uring: IoUringConfig::default() }
    }

    /// Returns current [`IoUringConfig`] of this [`IoWorkerConfig`].
    pub const fn io_uring_config(&self) -> IoUringConfig {
        self.io_uring
    }

    /// Checks if [`IoWorkerConfig`] is valid.
    pub const fn validate(&self) -> Result<(), &'static str> {
        self.io_uring.validate()
    }
}