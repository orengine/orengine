use std::task::{RawWaker, RawWakerVTable, Waker};
use crate::runtime::{local_executor};
use crate::runtime::task::Task;

/// This is really unsafe. [`Task`] has no some ref counters, so, task can be dropped before it is woken up.
/// And [`Task`] has no state, so it can be executed after [`Ready`](std::task::Poll::Ready) return.
///
/// Therefore, you need to make sure that [`Task`] is not called after drop or [`Ready`](std::task::Poll::Ready) return.
unsafe fn clone(data_ptr: *const ()) -> RawWaker {
    RawWaker::new(data_ptr, &VTABLE)
}

/// Do the same as [`wake_by_ref`].
/// [`Executor`](crate::runtime::Executor) will drop the [`Task`] only when it is needed.
/// So, you can call it without fear.
unsafe fn wake(data_ptr: *const ()) {
    local_executor().local_queue().push_front(unsafe { (data_ptr as *mut Task).read() });
}

/// Wakes the [`Task`].
/// [`Executor`](crate::runtime::Executor) will drop the [`Task`] only when it is needed.
/// So, you can call it without fear.
unsafe fn wake_by_ref(data_ptr: *const ()) {
    local_executor().local_queue().push_front(unsafe { (data_ptr as *mut Task).read() });
}

/// Do nothing, because [`Executor`](crate::runtime::Executor) will drop the [`Task`] only when it is needed.
unsafe fn drop(_: *const ()) {}

/// [`RawWakerVTable`] for `orengine` runtime only!
pub const VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);

/// Creates a [`Waker`] with [`orengine::VTABLE`](VTABLE).
#[inline(always)]
pub fn create_waker(data_ptr: *const ()) -> Waker {
    unsafe { Waker::from_raw(RawWaker::new(data_ptr, &VTABLE)) }
}
