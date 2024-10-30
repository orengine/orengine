use crate::runtime::local_executor;
use crate::runtime::task::Task;
use std::sync::Arc;
use std::task::{RawWaker, RawWakerVTable, Waker};

/// This is really unsafe. [`Task`] has no some ref counters, so, task can be dropped before it is woken up.
/// And [`Task`] has no state, so it can be executed after [`Ready`](std::task::Poll::Ready) return.
///
/// Therefore, you need to make sure that [`Task`] is not called after drop or [`Ready`](std::task::Poll::Ready) return.
unsafe fn clone(data_ptr: *const ()) -> RawWaker {
    #[cfg(debug_assertions)]
    {
        let task = unsafe { &*(data_ptr as *mut Task) };
        unsafe { Arc::increment_strong_count(&task.ref_count) };
    }
    RawWaker::new(data_ptr, &VTABLE)
}

macro_rules! generate_wake {
    ($data_ptr:expr) => {
        let task = unsafe { ($data_ptr as *mut Task).read() };
        if task.is_local() {
            local_executor().spawn_local_task(task);
        } else {
            local_executor().spawn_global_task(task);
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

/// Do nothing, because [`Executor`](crate::runtime::Executor) will drop the [`Task`] only when it is needed.
#[allow(unused)] // because #[cfg(debug_assertions)]
unsafe fn drop(data_ptr: *const ()) {
    #[cfg(debug_assertions)]
    {
        let task = unsafe { &*(data_ptr as *mut Task) };
        unsafe { Arc::decrement_strong_count(&task.ref_count) };
    }
}

/// [`RawWakerVTable`] for `orengine` runtime only!
pub const VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);

/// Creates a [`Waker`] with [`orengine::VTABLE`](VTABLE).
#[inline(always)]
pub fn create_waker(data_ptr: *const ()) -> Waker {
    unsafe { Waker::from_raw(RawWaker::new(data_ptr, &VTABLE)) }
}

// TODO test
