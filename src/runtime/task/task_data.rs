use crate::runtime::Locality;
#[cfg(target_pointer_width = "64")]
use crate::runtime::{IS_LOCAL_MASK, TASK_MASK};
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
    #[inline]
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
    #[inline]
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
    #[inline]
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

    /// Sets locality for the `TaskData`.
    #[inline]
    pub(crate) fn set_locality(&mut self, locality: Locality) {
        #[cfg(not(target_pointer_width = "64"))]
        {
            self.is_local = locality.value;
        }

        #[cfg(target_pointer_width = "64")]
        #[allow(clippy::transmute_undefined_repr, reason = "dark magic")]
        {
            let future_tagged_ptr = unsafe {
                std::mem::transmute::<*mut dyn Future<Output = ()>, i128>(self.future_tagged_ptr)
            };

            let tagged_ptr = future_tagged_ptr & !IS_LOCAL_MASK;
            let tagged_ptr = tagged_ptr | locality.value;

            #[allow(clippy::useless_transmute, reason = "false positive")]
            {
                self.future_tagged_ptr = unsafe {
                    std::mem::transmute::<i128, *mut dyn Future<Output = ()>>(tagged_ptr)
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::runtime::Task;
    use crate::{local_executor, Local};

    #[orengine::test::test_local]
    fn test_task_data() {
        let res = Local::new(false);
        let res_clone = res.clone();
        let mut task = unsafe {
            Task::from_future(
                async move {
                    *res_clone.borrow_mut() = true;
                },
                Locality::local(),
            )
        };

        assert!(task.data.is_local());
        assert!(task.is_local());

        task.data.set_locality(Locality::shared());

        assert!(!task.data.is_local());
        assert!(!task.is_local());

        local_executor().exec_task(task);

        assert!(*res.borrow());
    }
}
