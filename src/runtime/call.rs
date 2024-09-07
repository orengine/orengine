use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use crossbeam::utils::CachePadded;
use crate::atomic_task_queue::AtomicTaskList;

#[derive(Debug, Eq, PartialEq)]
pub enum Call {
    None,
    PushCurrentTaskTo(*const AtomicTaskList),
    PushCurrentTaskToAndRemoveItIfCounterIsZero(*const AtomicTaskList, *const AtomicUsize, Ordering),
    /// Stores `false` for the given `AtomicBool` with [`Release`] ordering.
    ReleaseAtomicBool(*const CachePadded<AtomicBool>)
}

impl Call {
    #[inline(always)]
    pub fn is_none(&self) -> bool {
        matches!(self, Call::None)
    }
}

impl Default for Call {
    fn default() -> Self {
        Call::None
    }
}
