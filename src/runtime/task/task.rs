use crate::runtime::task::task_data::TaskData;
use crate::runtime::task_pool;
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
    pub fn from_future<F: Future<Output = ()>>(future: F, is_local: usize) -> Self {
        assert!(is_local < 2, "is_local must be 0 or 1");
        task_pool().acquire(future, is_local)
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

    /// Drops the wrapped future.
    #[inline(always)]
    pub unsafe fn drop_future(&mut self) {
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
                panic!(
                    "[BUG] Local task has been moved to another executor.\
                    Please report it. Provide details about the place where the problem occurred\
                    and the conditions under which it happened. \
                    Thank you for helping us make orengine better!"
                );
            }
        }
    };
}

#[macro_export]
macro_rules! panic_if_local_in_future {
    ($cx:expr, $name_of_future:expr) => {
        #[cfg(debug_assertions)]
        {
            let task = unsafe { &mut *($cx.waker().data() as *mut crate::runtime::Task) };
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
