use crate::runtime::{Locality, IS_LOCAL_MASK, TASK_MASK};
use std::future::Future;

/// `*mut dyn Future<Output = ()>` and the [`locality`](Locality) information associated
/// with the [`Task`](crate::runtime::Task).
///
/// In systems with 64-bit pointers, the high bit is reserved for the task-locality flag
/// caused by the fact that `*mut dyn` can be safely cast to `i128`.
///
/// In systems with 32-bit pointers or 16-bit pointers, an extra `bool` is used.
#[derive(Clone, Copy)]
pub(crate) struct TaskData {
    #[cfg(not(target_pointer_width = "64"))]
    future_ptr: *mut dyn Future<Output = ()>,
    #[cfg(not(target_pointer_width = "64"))]
    is_local: bool,
    #[cfg(target_pointer_width = "64")]
    future_tagged_ptr: *mut dyn Future<Output = ()>,
}

impl TaskData {
    /// Creates a new `TaskData`.
    #[inline(always)]
    pub(crate) fn new(future: *mut dyn Future<Output = ()>, locality: Locality) -> Self {
        #[cfg(not(target_pointer_width = "64"))]
        return Self {
            future_ptr: future,
            is_local: locality.value,
        };

        #[cfg(target_pointer_width = "64")]
        #[allow(clippy::transmute_undefined_repr, reason = "dark magic")]
        {
            let mut tagged_ptr =
                unsafe { std::mem::transmute::<*mut dyn Future<Output = ()>, i128>(future) };

            tagged_ptr |= locality.value;

            #[allow(clippy::useless_transmute, reason = "false positive")]
            Self {
                future_tagged_ptr: unsafe {
                    std::mem::transmute::<i128, *mut dyn Future<Output = ()>>(tagged_ptr)
                },
            }
        }
    }

    /// Returns the future pointer associated with the `TaskData`.
    #[inline(always)]
    pub(crate) fn future_ptr(&self) -> *mut dyn Future<Output = ()> {
        #[cfg(not(target_pointer_width = "64"))]
        return self.future_ptr;

        #[cfg(target_pointer_width = "64")]
        #[allow(clippy::transmute_undefined_repr, reason = "dark magic")]
        {
            let future_tagged_ptr = unsafe {
                std::mem::transmute::<*mut dyn Future<Output = ()>, i128>(self.future_tagged_ptr)
            };

            #[allow(clippy::useless_transmute, reason = "false positive")]
            unsafe {
                std::mem::transmute::<i128, *mut dyn Future<Output = ()>>(
                    future_tagged_ptr & TASK_MASK,
                )
            }
        }
    }

    /// Returns whether the `TaskData` is local or not.
    #[inline(always)]
    pub(crate) fn is_local(&self) -> bool {
        #[cfg(not(target_pointer_width = "64"))]
        return self.is_local;

        #[cfg(target_pointer_width = "64")]
        #[allow(clippy::transmute_undefined_repr, reason = "dark magic")]
        {
            let future_tagged_ptr = unsafe {
                std::mem::transmute::<*mut dyn Future<Output = ()>, i128>(self.future_tagged_ptr)
            };

            (future_tagged_ptr & IS_LOCAL_MASK) != 0
        }
    }
}
