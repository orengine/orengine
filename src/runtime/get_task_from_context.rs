/// Returns a [`Task`] from a [`Context`](std::task::Context).
///
/// # Safety
///
/// If called with [`Context`](std::task::Context) with an `orengine` waker.
#[macro_export]
macro_rules! get_task_from_context {
    ($ctx:expr) => {
        std::ptr::read($ctx.waker().data().cast::<$crate::runtime::task::Task>())
    };
}
