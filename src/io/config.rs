use crate::io::sys::{FallbackConfig, IOUringConfig};

/// Config for `io worker`.
#[derive(Clone, Copy)]
pub struct IoWorkerConfig {
    /// Config for `IOUringWorker`.
    ///
    /// Read [`IOUringConfig`] for more details.
    pub io_uring: IOUringConfig,
    /// Config for `FallbackWorker`.
    ///
    /// Read [`FallbackConfig`] for more details.
    pub fallback: FallbackConfig,
    /// Number of __fixed__ buffers.
    pub number_of_fixed_buffers: u16,
}

impl IoWorkerConfig {
    /// Returns default [`IoWorkerConfig`]
    pub const fn default() -> Self {
        Self {
            io_uring: IOUringConfig::default(),
            fallback: FallbackConfig::default(),
            number_of_fixed_buffers: 16,
        }
    }

    /// Checks if [`IoWorkerConfig`] is valid.
    pub const fn validate(&self) -> Result<(), &'static str> {
        if let Err(err) = self.io_uring.validate() {
            return Err(err);
        }

        if let Err(err) = self.fallback.validate() {
            return Err(err);
        }

        Ok(())
    }
}
