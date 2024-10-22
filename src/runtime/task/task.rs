use crate::runtime::task::task_data::TaskData;
use crate::runtime::{task_pool, Locality};
use std::future::Future;

/// `Task` is a wrapper of a future.
#[derive(Copy, Clone)]
pub struct Task {
    pub(crate) data: TaskData,
    #[cfg(debug_assertions)]
    pub(crate) executor_id: usize,
}

impl Task {
    /// Returns a [`Task`] with the given future.
    ///
    /// # Panics
    ///
    /// If `is_local` is not `0` or `1`.
    #[inline(always)]
    pub fn from_future<F: Future<Output = ()>>(future: F, locality: Locality) -> Self {
        task_pool().acquire(future, locality)
    }

    /// Returns the future that are wrapped by this [`Task`].
    ///
    /// # Safety
    ///
    /// It is safe because it returns a pointer without dereferencing it.
    ///
    /// Deref it only if you know what you are doing.
    #[inline(always)]
    pub fn future_ptr(self) -> *mut dyn Future<Output = ()> {
        self.data.future_ptr()
    }

    #[inline(always)]
    pub fn is_local(self) -> bool {
        self.data.is_local()
    }

    /// Releases the wrapped future.
    #[inline(always)]
    pub unsafe fn release_future(&mut self) {
        task_pool().put(self.data.future_ptr());
    }
}

unsafe impl Send for Task {}

#[macro_export]
macro_rules! check_task_local_safety {
    ($task:expr) => {
        #[cfg(debug_assertions)]
        {
            if $task.is_local() && crate::local_executor().id() != $task.executor_id {
                if cfg!(test) && $task.executor_id == usize::MAX {
                    // All is ok
                    $task.executor_id = crate::local_executor().id();
                } else {
                    panic!(
                        "[BUG] Local task has been moved to another executor.\
                        Please report it. Provide details about the place where the problem occurred \
                        and the conditions under which it happened. \
                        Thank you for helping us make orengine better!"
                    );
                }
            }
        }
    };
}

#[macro_export]
macro_rules! panic_if_local_in_future {
    ($cx:expr, $name_of_future:expr) => {
        #[cfg(debug_assertions)]
        {
            let task = crate::get_task_from_context!($cx);
            if task.is_local() {
                panic!(
                    "You cannot call a local task in {}, because it can be moved! \
                    Use global task instead or use local structures if it is possible.",
                    $name_of_future
                );
            }
        }
    };
}
