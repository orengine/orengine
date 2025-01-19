use crate::runtime::Locality;
use crate::sync_task_queue::SyncTaskList;
use crossbeam::utils::CachePadded;
use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
    ///
    /// # Safety
    ///
    /// * task must return [`Poll::Pending`](std::task::Poll::Pending) immediately after calling this function
    ///
    /// * calling task must be shared (else you don't need any [`Calls`](Call))
    PushCurrentTaskAtTheStartOfLIFOSharedQueue,
    /// Pushes current task to the given `AtomicTaskList`.
    ///
    /// # Safety
    ///
    /// * `send_to` must be a valid pointer to [`SyncTaskQueue`](SyncTaskList)
    ///
    /// * the reference must live at least as long as this state of the task
    ///
    /// * task must return [`Poll::Pending`](std::task::Poll::Pending) immediately after calling this function
    ///
    /// * calling task must be shared (else you don't need any [`Calls`](Call))
    PushCurrentTaskTo(*const SyncTaskList),
    /// Pushes current task to the given `AtomicTaskList` and removes it if the given `AtomicUsize`
    /// is `0` with given `Ordering` after removing executes it.
    ///
    /// # Safety
    ///
    /// * `send_to` must be a valid pointer to [`SyncTaskQueue`](SyncTaskList)
    ///
    /// * task must return [`Poll::Pending`](std::task::Poll::Pending) immediately after calling this function
    ///
    /// * counter must be a valid pointer to [`AtomicUsize`]
    ///
    /// * the references must live at least as long as this state of the task
    ///
    /// * calling task must be shared (else you don't need any [`Calls`](Call))
    PushCurrentTaskToAndRemoveItIfCounterIsZero(*const SyncTaskList, *const AtomicUsize, Ordering),
    /// Stores `false` for the given `AtomicBool` with [`Release`](Ordering::Release) ordering.
    ///
    /// # Safety
    ///
    /// * `atomic_bool` must be a valid pointer to [`AtomicBool`]
    ///
    /// * the [`AtomicBool`] must live at least as long as this state of the task
    ///
    /// * task must return [`Poll::Pending`](std::task::Poll::Pending) immediately after calling this function
    ///
    /// * calling task must be shared (else you don't need any [`Calls`](Call))
    ReleaseAtomicBool(*const CachePadded<AtomicBool>),
    /// Pushes `f` to the blocking pool.
    ///
    /// # Safety
    ///
    /// * the [`Fn`] must live at least as long as this state of the task.
    ///
    /// * task must return [`Poll::Pending`](std::task::Poll::Pending) immediately after calling this function
    ///
    /// * calling task must be shared (else you don't need any [`Calls`](Call))
    PushFnToThreadPool(*mut dyn Fn()),
    /// Changes current task locality and wakes up current task.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::future::Future;
    /// use std::pin::Pin;
    /// use std::task::{Context, Poll};
    /// use orengine::local_executor;
    /// use orengine::runtime::call::Call;
    /// use orengine::runtime::Locality;
    ///
    /// struct UpdateCurrentTaskLocality {
    ///     locality: Locality,
    ///     was_called: bool,
    /// }
    ///
    /// impl Future for UpdateCurrentTaskLocality {
    ///     type Output = ();
    ///
    ///     fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
    ///         let this = unsafe { self.get_unchecked_mut() };
    ///
    ///         if !this.was_called {
    ///             this.was_called = true;
    ///
    ///             unsafe {
    ///                 local_executor().invoke_call(Call::ChangeCurrentTaskLocality(this.locality));
    ///             };
    ///
    ///             return Poll::Pending;
    ///         }
    ///
    ///         Poll::Ready(())
    ///     }
    /// }
    /// ```
    ChangeCurrentTaskLocality(Locality),
}

impl Call {
    /// Returns `true` if the `Call` is [`None`](Self::None).
    #[inline]
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
            Self::ChangeCurrentTaskLocality(locality) => write!(
                f,
                "Call::ChangeCurrentTaskLocality with locality: {locality:?}"
            ),
        }
    }
}
