use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};
use crate::messages::BUG;
use crate::utils::{get_core_ids, CoreId};

/// A list of cores ids. It is needed for balancing the [`executors`](crate::Executor) between cores.
static CORES_IDS_LIST: LazyLock<Mutex<VecDeque<CoreId>>> = LazyLock::new(|| {
    let cores = get_core_ids().expect(BUG);
    Mutex::new(VecDeque::from(cores))
});

/// Returns core id for executor. It is already balanced between cores.
#[inline(always)]
pub fn get_core_id_for_executor() -> CoreId {
    let mut table = CORES_IDS_LIST.lock().expect(BUG);
    let core_id = table.pop_front().expect(BUG);
    table.push_back(core_id);

    core_id
}