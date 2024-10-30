use crate::runtime::task::task_data::TaskData;
use crate::runtime::{task_pool, Locality};
use std::future::Future;
use std::sync::Arc;

/// `Task` is a wrapper of a future.
pub struct Task {
    pub(crate) data: TaskData,
    #[cfg(debug_assertions)]
    pub(crate) executor_id: usize,
    #[cfg(debug_assertions)]
    pub(crate) ref_count: Arc<()>,
}

impl Task {
    /// Returns a [`Task`] with the given future.
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
    /// Deref it only if you know what you are doing and remember that [`Task`] can be executed
    /// only by [`Executor::exec_task`](crate::Executor::exec_task) and it can't be cloned, only moved.
    #[inline(always)]
    pub fn future_ptr(&self) -> *mut dyn Future<Output = ()> {
        self.data.future_ptr()
    }

    #[inline(always)]
    pub fn is_local(&self) -> bool {
        self.data.is_local()
    }

    /// Releases the wrapped future.
    #[inline(always)]
    pub unsafe fn release_future(&mut self) {
        task_pool().put(self.data.future_ptr());
    }

    /// Checks if the task is safe to be executed.
    /// It checks ref_count and executor_id with locality.
    ///
    /// It is zero cost because it can be called only in debug mode.
    #[cfg(debug_assertions)]
    pub(crate) fn check_safety(&mut self) {
        if Arc::strong_count(&self.ref_count) != 1 {
            panic!(
                "Task has been cloned! But it can be only moved. Tasks are implement ownership \
            semantics. This could only happen in Waker::clone. Waker::clone is allowed only \
            for moving. You must drop and old waker before task is executed next time."
            );
        }

        if self.is_local() && self.executor_id != crate::local_executor().id() {
            if cfg!(test) && self.executor_id == usize::MAX {
                // All is ok
                self.executor_id = crate::local_executor().id();
            } else {
                panic!("Local task has been moved to another executor!");
            }
        }
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
