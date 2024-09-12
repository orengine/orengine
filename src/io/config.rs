use crate::io::sys::unix::IoUringConfig;

#[derive(Clone, Copy)]
pub struct IoWorkerConfig {
    pub(crate) io_uring: IoUringConfig
}

impl IoWorkerConfig {
    pub const fn default() -> Self {
        Self { io_uring: IoUringConfig::default() }
    }

    pub const fn io_uring_config(&self) -> IoUringConfig {
        self.io_uring
    }

    pub const fn validate(&self) -> Result<(), &'static str> {
        self.io_uring.validate()
    }
}