// TODO: update code to std::task::Waker::data when it will be stabilized
use crate::runtime::Task;
use std::task::RawWakerVTable;

/// The clone of [`Waker`](std::task::Waker) with public `data` field.
pub struct WakerWithPubData {
    data: *const (),
    #[allow(dead_code)]
    vtable: &'static RawWakerVTable,
}

impl WakerWithPubData {
    /// Returns a [`Task`] that is wrapped by the [`Waker`](std::task::Waker).
    ///
    /// If `debug_assertions` is enabled, it also increments the reference count.
    ///
    /// # Safety
    ///
    /// [`Wakers`](std::task::Waker) contains `*const Task` in `data` field.
    #[inline(always)]
    pub unsafe fn data(&self) -> Task {
        unsafe { std::ptr::read(self.data.cast::<Task>()) }
    }
}

/// Returns a [`Task`] from a [`Context`](std::task::Context).
///
/// # Safety
///
/// If called with [`Context`](std::task::Context) with an `orengine` waker.
#[macro_export]
macro_rules! get_task_from_context {
    ($ctx:expr) => {
        {
            let waker_ref = &*(
                std::ptr::from_ref($ctx.waker())
                .cast::<$crate::runtime::get_task_from_context::WakerWithPubData>()
            );

            waker_ref.data()
        }
    };
}
