use std::future::Future;
use crate::runtime::task_pool::{TASK_POOL, task_pool};

#[derive(Copy, Clone)]
pub struct Task {
    pub(crate) future_ptr: *mut dyn Future<Output=()>,
}

static A: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl Task {
    pub fn from_future<F: Future<Output=()> + 'static>(future: F) -> Self {
        Self {
            future_ptr: unsafe { task_pool().acquire(future) }
        }
    }

    pub unsafe fn drop_future(&mut self) {
        unsafe { task_pool().put(self.future_ptr) }
    }
}