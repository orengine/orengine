use crate::io::sys::unix::IOUringConfig;

/// Config for `io worker`.
#[derive(Clone, Copy)]
pub struct IoWorkerConfig {
    /// Config for `IOUringWorker`.
    ///
    /// Read [`IOUringConfig`] for more details.
    pub io_uring: IOUringConfig,
    /// Number of __fixed__ buffers.
    pub number_of_fixed_buffers: u16,
}

impl IoWorkerConfig {
    /// Returns default [`IoWorkerConfig`]
    pub const fn default() -> Self {
        Self {
            io_uring: IOUringConfig::default(),
            number_of_fixed_buffers: 16,
        }
    }

    /// Checks if [`IoWorkerConfig`] is valid.
    pub const fn validate(&self) -> Result<(), &'static str> {
        self.io_uring.validate()
    }
}
