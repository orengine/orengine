use crate::buf::buf_pool;

pub const DEFAULT_BUF_LEN: usize = 4096;

/// The configuration of the scheduler.
pub struct ExecuterCfg {
    pub(crate) buf_len: usize
}

impl ExecuterCfg {
    /// Creates new [`ExecuterCfg`] with default values.
    /// We can't use [`Default`] trait, because [`Default::default`] is not `const fn`.
    pub const fn default() -> Self {
        Self {
            buf_len: DEFAULT_BUF_LEN
        }
    }
}

/// The configuration of the executor.
/// This will be read only on [`run_on_core`](crate::run::run_on_core) and [`run_on_all_cores`](crate::run::run_on_all_cores).
pub(crate) static mut EXECUTER_CFG: ExecuterCfg = ExecuterCfg::default();

/// Getter for [`SCHEDULER_CFG::buf_len`].
pub fn config_buf_len() -> usize {
    unsafe { EXECUTER_CFG.buf_len }
}

/// Setter for [`SCHEDULER_CFG::buf_len`].
#[allow(dead_code)]
pub fn set_buf_len(buf_len: usize) {
    unsafe { EXECUTER_CFG.buf_len = buf_len }
    buf_pool().tune_buffer_len(buf_len);
}

/// Setter for [`EXECUTER_CFG`].
#[allow(dead_code)]
pub fn set_config(config: ExecuterCfg) {
    buf_pool().tune_buffer_len(config.buf_len);
    unsafe { EXECUTER_CFG = config };
}