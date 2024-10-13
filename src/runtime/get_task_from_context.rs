// TODO docs
// FIXME: update code to std::task::Waker::data when it will be stabilized
use std::task::RawWakerVTable;

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
            let waker_ptr = std::mem::transmute::<
                &std::task::Waker,
                &crate::runtime::get_task_from_context::WakerWithPubData,
            >($ctx.waker());
            std::ptr::read(waker_ptr.data as *const _ as *mut crate::runtime::Task)
        }
    };
}
