use crate::utils::{get_core_ids, CoreId};
use crate::BUG_MESSAGE;
use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};

/// A list of cores ids. It is needed for balancing the [`executors`](crate::Executor) between cores.
static CORES_IDS_LIST: LazyLock<Mutex<VecDeque<CoreId>>> = LazyLock::new(|| {
    let cores = get_core_ids().expect(BUG_MESSAGE);
    Mutex::new(VecDeque::from(cores))
});

/// Returns core id for executor. It is already balanced between cores.
#[inline(always)]
#[allow(
    clippy::missing_panics_doc,
    reason = "It panics only when a bug is occurred."
)]
pub fn get_core_id_for_executor() -> CoreId {
    let mut table = CORES_IDS_LIST.lock().expect(BUG_MESSAGE);
    let core_id = table.pop_front().expect(BUG_MESSAGE);
    table.push_back(core_id);

    core_id
}
