use crate::runtime::{Locality, IS_LOCAL_MASK, TASK_MASK};
// TODO docs
use std::future::Future;

#[cfg(not(target_pointer_width = "64"))]
#[derive(Clone, Copy)]
pub(crate) struct TaskData {
    future_ptr: *mut dyn Future<Output = ()>,
    is_local: bool,
}

#[cfg(target_pointer_width = "64")]
#[derive(Clone, Copy)]
pub(crate) struct TaskData {
    future_tagged_ptr: *mut dyn Future<Output = ()>,
}

impl TaskData {
    /// Creates a new `TaskData`.
    ///
    /// # Panics
    ///
    /// If `is_local` is not `0` or `1`.
    #[inline(always)]
    pub(crate) fn new(future: *mut dyn Future<Output = ()>, locality: Locality) -> Self {
        #[cfg(not(target_pointer_width = "64"))]
        return Self {
            future_ptr: future,
            is_local: locality.value,
        };

        #[cfg(any(target_pointer_width = "64"))]
        {
            let mut tagged_ptr =
                unsafe { std::mem::transmute::<*mut dyn Future<Output = ()>, i128>(future) };

            tagged_ptr |= locality.value;

            Self {
                future_tagged_ptr: unsafe {
                    std::mem::transmute::<i128, *mut dyn Future<Output = ()>>(tagged_ptr)
                },
            }
        }
    }

    #[inline(always)]
    pub(crate) fn future_ptr(&self) -> *mut dyn Future<Output = ()> {
        #[cfg(not(target_pointer_width = "64"))]
        return self.future_ptr;

        #[cfg(any(target_pointer_width = "64"))]
        {
            let future_tagged_ptr = unsafe {
                std::mem::transmute::<*mut dyn Future<Output = ()>, i128>(self.future_tagged_ptr)
            };

            unsafe {
                std::mem::transmute::<i128, *mut dyn Future<Output = ()>>(
                    future_tagged_ptr & TASK_MASK,
                )
            }
        }
    }

    #[inline(always)]
    pub(crate) fn is_local(&self) -> bool {
        #[cfg(not(target_pointer_width = "64"))]
        return self.is_local;

        #[cfg(any(target_pointer_width = "64"))]
        {
            let future_tagged_ptr = unsafe {
                std::mem::transmute::<*mut dyn Future<Output = ()>, i128>(self.future_tagged_ptr)
            };

            (future_tagged_ptr & IS_LOCAL_MASK) != 0
        }
    }
}
