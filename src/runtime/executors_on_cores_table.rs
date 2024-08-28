use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};
use crate::messages::BUG;
use crate::utils::{get_core_ids, CoreId};

static EXECUTORS_ON_CORES_TABLE: LazyLock<Mutex<VecDeque<CoreId>>> = LazyLock::new(|| {
    let cores = get_core_ids().expect(BUG);
    Mutex::new(VecDeque::from(cores))
});

#[inline(always)]
pub fn get_core_id_for_executor() -> CoreId {
    let mut table = EXECUTORS_ON_CORES_TABLE.lock().expect(BUG);
    let core_id = table.pop_front().expect(BUG);
    table.push_back(core_id);

    core_id
}