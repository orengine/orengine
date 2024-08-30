use std::future::Future;
use crate::runtime::task_pool::task_pool;

#[derive(Copy, Clone)]
pub struct Task {
    pub(crate) future_ptr: *mut dyn Future<Output=()>,
}

impl Task {
    pub fn from_future<F: Future<Output=()>>(future: F) -> Self {
        task_pool().acquire(future)
    }

    pub unsafe fn drop_future(&mut self) {
        task_pool().put(self.future_ptr)
    }
}