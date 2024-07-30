/// The configuration of the scheduler.
pub struct ExecuterCfg {
    buf_len: usize
}

impl ExecuterCfg {
    /// Creates new [`ExecuterCfg`] with default values.
    /// We can't use [`Default`] trait, because [`Default::default`] is not `const fn`.
    pub const fn default() -> Self {
        Self {
            buf_len: 4096
        }
    }
}

/// The configuration of the executer.
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
}

/// Setter for [`EXECUTER_CFG`].
#[allow(dead_code)]
pub fn set_config(config: ExecuterCfg) {
    unsafe { EXECUTER_CFG = config }
}