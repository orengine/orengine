use crate::local_executor;
use crate::runtime::call::Call;
use crate::runtime::task::task_data::TaskData;
use crate::runtime::{task_pool, Locality};
use std::future::Future;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::pin::Pin;
use std::task::{Context, Poll};

/// `Task` is a wrapper of a future.
///
/// If `debug_assertions` is enabled, it keeps additional information to check
/// if the task is safe to be executed.
///
/// # Be careful
///
/// `Task` __must__ be executed via [`Executor::exec_task`](crate::Executor::exec_task).
pub struct Task {
    pub(crate) data: TaskData,
    #[cfg(debug_assertions)]
    pub(crate) executor_id: usize,
    #[cfg(debug_assertions)]
    pub(crate) is_executing: crate::utils::Ptr<std::sync::atomic::AtomicBool>,
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

    /// Returns whether the task is local or not.
    #[inline(always)]
    pub fn is_local(&self) -> bool {
        self.data.is_local()
    }

    /// Puts it back to the [`TaskPool`](crate::runtime::TaskPool). It is unsafe because you
    /// have to think about making sure it is no longer used.
    ///
    /// # Safety
    ///
    /// Provided [`Task`] is no longer used.
    #[inline(always)]
    pub(crate) unsafe fn release(self) {
        task_pool().put(self);
    }

    /// Checks if the task is safe to be executed.
    /// It checks `ref_count` and `executor_id` with locality.
    ///
    /// It is zero cost because it can be called only in debug mode.
    #[cfg(debug_assertions)]
    pub(crate) fn check_safety(&mut self) {
        if unsafe {
            self.is_executing
                .as_ref()
                .load(std::sync::atomic::Ordering::SeqCst)
        } {
            panic!(
                "Attempt to execute an already executing task! It is not allowed! \
            Try to rewrite the code to follow the concept of task ownership: \
            only one thread can own a task at the same time and only one task instance can exist."
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
impl UnwindSafe for Task {}
impl RefUnwindSafe for Task {}

/// In `debug` mode checks if the [`Task`] is safe to be executed.
///
/// It compares an id of the [`Executor`](crate::Executor) of the current thread with an id of the executor of
/// the [`Task`], if the [`Task`] is `local`.
#[macro_export]
macro_rules! check_task_local_safety {
    ($task:expr) => {
        #[cfg(debug_assertions)]
        {
            if $task.is_local() && $crate::local_executor().id() != $task.executor_id {
                if cfg!(test) && $task.executor_id == usize::MAX {
                    // All is ok
                    $task.executor_id = $crate::local_executor().id();
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

/// Gets the [`Task`] from the context and panics if it is `local`.
///
/// # Panics
///
/// If the [`Task`] associated with the context is `local`.
///
/// # Safety
///
/// Provided context contains a valid [`Task`] in `data` field (always true if you call it in
/// orengine runtime).
#[macro_export]
macro_rules! panic_if_local_in_future {
    ($cx:expr, $name_of_future:expr) => {
        #[cfg(debug_assertions)]
        #[allow(
            clippy::macro_metavars_in_unsafe,
            reason = "else we need to allow unused `unsafe` for `release`"
        )]
        unsafe {
            let task = $crate::get_task_from_context!($cx);
            if task.is_local() {
                panic!(
                    "You cannot call a local task in {}, because it can be moved! \
                    Use shared task instead or use local structures if it is possible.",
                    $name_of_future
                );
            }
        }
    };
}

/// Update current [`task`](Task) locality via [`calling`](crate::Executor::invoke_call)
/// [`ChangeCurrentTaskLocality`](Call::ChangeCurrentTaskLocality).
///
/// It is unsafe because you have to think about making sure
/// that current task can have provided locality. Use it only if you know what you are doing.
/// Maybe it is the most unsafe function in the whole crate.
///
/// # Safety
///
/// Provided `locality` is valid for the current [`task`](Task).
pub async unsafe fn update_current_task_locality(locality: Locality) {
    struct UpdateCurrentTaskLocality {
        locality: Locality,
        was_called: bool,
    }

    impl Future for UpdateCurrentTaskLocality {
        type Output = ();

        fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
            let this = unsafe { self.get_unchecked_mut() };

            if !this.was_called {
                this.was_called = true;

                unsafe {
                    local_executor().invoke_call(Call::ChangeCurrentTaskLocality(this.locality));
                };

                return Poll::Pending;
            }

            Poll::Ready(())
        }
    }

    UpdateCurrentTaskLocality {
        locality,
        was_called: false,
    }
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::get_task_from_context;

    #[orengine::test::test_local]
    fn test_update_current_task_locality() {
        struct GetCurrentTaskLocality {}

        impl Future for GetCurrentTaskLocality {
            type Output = bool;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let task = unsafe { get_task_from_context!(cx) };

                Poll::Ready(task.is_local())
            }
        }

        assert!(GetCurrentTaskLocality {}.await);

        unsafe {
            update_current_task_locality(Locality::shared()).await;
        }

        assert!(!GetCurrentTaskLocality {}.await);
    }
}
