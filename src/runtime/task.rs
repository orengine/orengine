use std::future::Future;
use crate::runtime::task_pool::task_pool;

#[derive(Copy, Clone)]
pub struct Task {
    pub(crate) future_ptr: *mut dyn Future<Output=()>,
}

impl Task {
    pub fn from_future<F: Future<Output=()> + 'static>(future: F) -> Self {
        Self {
            future_ptr: task_pool().acquire(future)
        }
    }

    pub unsafe fn drop_future(&mut self) {
        task_pool().put(self.future_ptr)
    }

    #[inline(always)]
    pub(crate) unsafe fn as_ptr(&self) -> *mut dyn Future<Output=()> {
        self.future_ptr
    }

    #[inline(always)]
    pub(crate) unsafe fn from_ptr(ptr: *mut dyn Future<Output=()>) -> Self {
        Self {
            future_ptr: ptr
        }
    }
}