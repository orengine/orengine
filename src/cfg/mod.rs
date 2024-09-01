use crate::buf::buf_pool;

pub const DEFAULT_BUF_LEN: usize = 4096;

/// The configuration of the scheduler.
pub struct ExecutorCfg {
    pub(crate) buf_len: usize,
}

impl ExecutorCfg {
    /// Creates new [`ExecutorCfg`] with default values.
    /// We can't use [`Default`] trait, because [`Default::default`] is not `const fn`.
    pub const fn default() -> Self {
        Self {
            buf_len: DEFAULT_BUF_LEN,
        }
    }
}

/// The configuration of the executor.
/// This will be read only on [`run_on_core`](crate::run::run_on_core) and [`run_on_all_cores`](crate::run::run_on_all_cores).
pub(crate) static mut EXECUTOR_CFG: ExecutorCfg = ExecutorCfg::default();

/// Getter for [`SCHEDULER_CFG::buf_len`].
pub fn config_buf_len() -> usize {
    unsafe { EXECUTOR_CFG.buf_len }
}

/// Setter for [`SCHEDULER_CFG::buf_len`].
#[allow(dead_code)]
pub fn set_buf_len(buf_len: usize) {
    unsafe { EXECUTOR_CFG.buf_len = buf_len }
    buf_pool().tune_buffer_len(buf_len);
}

/// Setter for [`EXECUTOR_CFG`].
#[allow(dead_code)]
pub fn set_config(config: ExecutorCfg) {
    buf_pool().tune_buffer_len(config.buf_len);
    unsafe { EXECUTOR_CFG = config };
}
