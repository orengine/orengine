use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crossbeam::utils::CachePadded;

use crate::sync_task_queue::SyncTaskList;

/// Represents a call from a `Future::poll` to the [`Executor`](crate::runtime::Executor).
///
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
#[derive(Default)]
pub enum Call {
    /// Does nothing
    #[default]
    None,
    /// Moves current `shared` task at the start of a shared `LIFO`
    /// task queue of the current executor.
    PushCurrentTaskAtTheStartOfLIFOSharedQueue,
    /// Pushes current task to the given `AtomicTaskList`.
    PushCurrentTaskTo(*const SyncTaskList),
    /// Pushes current task to the given `AtomicTaskList` and removes it if the given `AtomicUsize`
    /// is `0` with given `Ordering` after removing executes it.
    PushCurrentTaskToAndRemoveItIfCounterIsZero(*const SyncTaskList, *const AtomicUsize, Ordering),
    /// Stores `false` for the given `AtomicBool` with [`Release`](Ordering::Release) ordering.
    ReleaseAtomicBool(*const CachePadded<AtomicBool>),
    /// Pushes `f` to the blocking pool.
    ///
    /// Provided `Fn()` must have `'static` lifetime
    PushFnToThreadPool(*mut dyn Fn()),
}

impl Call {
    #[inline(always)]
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
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
            Self::None => write!(f, "Call::None"),
            Self::PushCurrentTaskAtTheStartOfLIFOSharedQueue => {
                write!(f, "Call::YieldCurrentSharedTask")
            }
            Self::PushCurrentTaskTo(_) => write!(f, "Call::PushCurrentTaskTo"),
            Self::PushCurrentTaskToAndRemoveItIfCounterIsZero(_, _, _) => {
                write!(f, "Call::PushCurrentTaskToAndRemoveItIfCounterIsZero")
            }
            Self::ReleaseAtomicBool(_) => write!(f, "Call::ReleaseAtomicBool"),
            Self::PushFnToThreadPool(_) => write!(f, "Call::PushFnToThreadPool"),
        }
    }
}
