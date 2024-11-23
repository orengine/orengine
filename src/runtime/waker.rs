use crate::runtime::local_executor;
use crate::runtime::task::Task;
use std::task::{RawWaker, RawWakerVTable, Waker};

/// This is really unsafe.
///
/// - [`Task`] has no some ref counters, so, task can be dropped
///   before it is woken up.
///
/// - [`Task`] can be executed only when it is not running. So you need to control that
///   [`Task`] never be woken up while it is running. Read [`Call`](crate::runtime::call::Call)
///   for more details.
///
/// # Safety
///
/// - [`Task`] never be executed after
///   it was dropped (after it returned [`Poll::Ready`](std::task::Poll::Ready)).
///
/// - [`Task`] never executed while it is running.
unsafe fn clone(data_ptr: *const ()) -> RawWaker {
    RawWaker::new(data_ptr, &VTABLE)
}

/// Wakes the [`Task`] considering whether it is `local` or `shared`.
macro_rules! generate_wake {
    ($data_ptr:expr) => {
        let task = unsafe { ($data_ptr as *mut Task).read() };
        if task.is_local() {
            local_executor().spawn_local_task(task);
        } else {
            local_executor().spawn_shared_task(task);
        }
    };
}

/// Do the same as [`wake_by_ref`].
/// [`Executor`](crate::runtime::Executor) will drop the [`Task`] only when it is needed.
/// So, you can call it without fear.
unsafe fn wake(data_ptr: *const ()) {
    generate_wake!(data_ptr);
}

/// Wakes the [`Task`].
/// [`Executor`](crate::runtime::Executor) will drop the [`Task`] only when it is needed.
/// So, you can call it without fear.
unsafe fn wake_by_ref(data_ptr: *const ()) {
    generate_wake!(data_ptr);
}

/// Do nothing, because [`Executor`](crate::runtime::Executor) will drop the [`Task`]
/// only when it is needed.
#[allow(unused)] // because #[cfg(debug_assertions)]
unsafe fn drop(data_ptr: *const ()) {}

/// [`RawWakerVTable`] for `orengine` runtime only!
pub const VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);

/// Creates a [`Waker`] with [`orengine::VTABLE`](VTABLE).
#[inline(always)]
pub fn create_waker(task_ptr: *mut Task) -> Waker {
    unsafe { Waker::from_raw(RawWaker::new(task_ptr as *const (), &VTABLE)) }
}
