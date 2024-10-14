// TODO: update code to std::task::Waker::data when it will be stabilized
use std::task::RawWakerVTable;

/// [`Waker`](std::task::Waker) with public `data` field.
pub struct WakerWithPubData {
    pub data: *const (),
    #[allow(dead_code)]
    vtable: &'static RawWakerVTable,
}

/// Returns a [`Task`] from a [`Context`](std::task::Context).
///
/// # Undefined behavior
///
/// If called on not a [`Context`](std::task::Context) with an `orengine` waker.
#[macro_export]
macro_rules! get_task_from_context {
    ($ctx:expr) => {
        unsafe {
            let waker_ptr = &*($ctx.waker() as *const _
                as *const crate::runtime::get_task_from_context::WakerWithPubData);
            std::ptr::read(waker_ptr.data as *const crate::runtime::Task)
        }
    };
}
