use crate::io::sys::unix::IOUringConfig;

/// Config for [`IoWorker`](crate::io::worker::IoWorker).
#[derive(Clone, Copy)]
pub struct IoWorkerConfig {
    /// Config for [`IOUringWorker`](crate::io::sys::unix::IOUringWorker).
    ///
    /// Read [`IOUringConfig`](IOUringConfig) for more details.
    pub(crate) io_uring: IOUringConfig,
}

impl IoWorkerConfig {
    /// Returns default [`IoWorkerConfig`]
    pub const fn default() -> Self {
        Self {
            io_uring: IOUringConfig::default(),
        }
    }

    /// Returns current [`IOUringConfig`] of this [`IoWorkerConfig`].
    pub const fn io_uring_config(&self) -> IOUringConfig {
        self.io_uring
    }

    /// Checks if [`IoWorkerConfig`] is valid.
    pub const fn validate(&self) -> Result<(), &'static str> {
        self.io_uring.validate()
    }
}
