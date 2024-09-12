use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use crossbeam::utils::CachePadded;
use crate::atomic_task_queue::AtomicTaskList;

pub enum Call {
    None,
    PushCurrentTaskTo(*const AtomicTaskList),
    PushCurrentTaskToAndRemoveItIfCounterIsZero(*const AtomicTaskList, *const AtomicUsize, Ordering),
    /// Stores `false` for the given `AtomicBool` with [`Release`] ordering.
    ReleaseAtomicBool(*const CachePadded<AtomicBool>),
    PushFnToThreadPool(&'static mut dyn Fn())
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

impl Eq for Call {}

impl PartialEq for Call {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

impl Debug for Call {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Call::None => write!(f, "Call::None"),
            Call::PushCurrentTaskTo(_) => write!(f, "Call::PushCurrentTaskTo"),
            Call::PushCurrentTaskToAndRemoveItIfCounterIsZero(_, _, _) => {
                write!(f, "Call::PushCurrentTaskToAndRemoveItIfCounterIsZero")
            },
            Call::ReleaseAtomicBool(_) => write!(f, "Call::ReleaseAtomicBool"),
            Call::PushFnToThreadPool(_) => write!(f, "Call::PushFnToThreadPool"),
        }
    }
}
