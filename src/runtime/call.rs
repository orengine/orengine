use std::sync::atomic::{AtomicUsize, Ordering};
use crate::atomic_task_queue::AtomicTaskList;

pub enum Call {
    None,
    PushCurrentTaskTo(*const AtomicTaskList),
    PushCurrentTaskToAndRemoveItIfCounterIsZero(*const AtomicTaskList, *const AtomicUsize, Ordering),
}

impl Default for Call {
    fn default() -> Self {
        Call::None
    }
}
