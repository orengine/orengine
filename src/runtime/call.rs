use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crossbeam::utils::CachePadded;

use crate::atomic_task_queue::AtomicTaskList;

/// Represents a call from a `Future::poll` to the [`Executor`](crate::runtime::Executor).
/// The `Call` enum encapsulates different actions that an executor can take
/// after a future yields [`Poll::Pending`](std::task::Poll::Pending).
/// These actions may involve scheduling
/// the current task, signaling readiness, or interacting with sync primitives.
///
/// # Safety
///
/// After invoking a `Call`, the associated future **must** return `Poll::Pending` immediately.
/// The action defined by the `Call` will be executed **after** the future returns, ensuring
/// that it is safe to perform state transitions or task scheduling without directly affecting
/// the current state of the future. Use this mechanism only if you fully understand the
/// implications and safety concerns of moving a future between different states or threads.
pub(crate) enum Call {
    /// Does nothing
    None,
    /// Moves current `global` task at the start of a global `LIFO`
    /// task queue of the current executor.
    PushCurrentTaskAtTheStartOfLIFOGlobalQueue,
    /// Pushes current task to the given `AtomicTaskList`.
    PushCurrentTaskTo(*const AtomicTaskList),
    /// Pushes current task to the given `AtomicTaskList` and removes it if the given `AtomicUsize`
    /// is `0` with given `Ordering` after removing executes it.
    PushCurrentTaskToAndRemoveItIfCounterIsZero(
        *const AtomicTaskList,
        *const AtomicUsize,
        Ordering,
    ),
    /// Stores `false` for the given `AtomicBool` with [`Release`] ordering.
    ReleaseAtomicBool(*const CachePadded<AtomicBool>),
    /// Pushes `f` to the blocking pool.
    PushFnToThreadPool(&'static mut dyn Fn()),
}

impl Call {
    #[inline(always)]
    pub(crate) fn is_none(&self) -> bool {
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
            Call::PushCurrentTaskAtTheStartOfLIFOGlobalQueue => {
                write!(f, "Call::YieldCurrentGlobalTask")
            }
            Call::PushCurrentTaskTo(_) => write!(f, "Call::PushCurrentTaskTo"),
            Call::PushCurrentTaskToAndRemoveItIfCounterIsZero(_, _, _) => {
                write!(f, "Call::PushCurrentTaskToAndRemoveItIfCounterIsZero")
            }
            Call::ReleaseAtomicBool(_) => write!(f, "Call::ReleaseAtomicBool"),
            Call::PushFnToThreadPool(_) => write!(f, "Call::PushFnToThreadPool"),
        }
    }
}
