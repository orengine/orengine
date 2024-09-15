use std::future::Future;
use crate::runtime::task_pool;

#[derive(Copy, Clone)]
pub struct Task {
    pub(crate) future_ptr: *mut dyn Future<Output=()>,
    #[cfg(debug_assertions)]
    pub(crate) executor_id: usize,
    #[cfg(debug_assertions)]
    pub(crate) is_local: bool
}

impl Task {
    pub fn from_future<F: Future<Output=()>>(future: F) -> Self {
        task_pool().acquire(future)
    }

    pub unsafe fn drop_future(&mut self) {
        task_pool().put(self.future_ptr)
    }
}

#[macro_export]
macro_rules! check_task_local_safety {
    ($task:expr) => {
        #[cfg(debug_assertions)]
        {
            if $task.is_local && crate::local_executor().id() != $task.executor_id {
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
            if task.is_local {
                panic!(
                    "You cannot call a local task in {}, because it can be moved! \
                    Use global task instead or use local structures if it is possible.",
                    $name_of_future
                );
            }
        }
    };
}