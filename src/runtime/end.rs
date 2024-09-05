use std::sync::LazyLock;
use dashmap::{DashSet};

static END_TABLE: LazyLock<DashSet<usize, ahash::RandomState>> = LazyLock::new(|| {
    DashSet::with_hasher(ahash::RandomState::new())
});

pub(crate) fn init_new_worker_end(worker_id: usize) {
    END_TABLE.insert(worker_id);
}

#[inline(always)]
pub(crate) fn is_ended_by_id(worker_id: usize) -> bool {
    !END_TABLE.contains(&worker_id)
}

#[inline(always)]
pub unsafe fn end_worker_by_id(worker_id: usize) {
    END_TABLE.remove(&worker_id);
}

#[inline(always)]
pub unsafe fn end_all_workers() {
    END_TABLE.clear();
}

#[cfg(test)]
mod tests {
    use crate::local_executor;
    use super::*;

    #[test_macro::test]
    fn test_end_local_worker() {
        let id = local_executor().worker_id();
        assert!(!is_ended_by_id(id));
        unsafe { end_worker_by_id(id) };
        assert!(is_ended_by_id(id));
    }

    #[test_macro::test]
    fn test_end_all_workers() {
        let id = local_executor().worker_id();
        assert!(!is_ended_by_id(id));
        unsafe { end_all_workers() };
        assert!(is_ended_by_id(id));
    }
}