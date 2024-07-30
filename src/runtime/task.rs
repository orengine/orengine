use std::future::Future;

#[derive(Copy, Clone)]
pub struct Task<T = ()> {
    pub(crate) future_ptr: *mut dyn Future<Output=T>,
}

impl<T> Task<T> {
    pub fn from_future<F: Future<Output=T> + 'static>(future: F) -> Self {
        Self {
            future_ptr: Box::leak(Box::new(future))
        }
    }

    pub unsafe fn drop_future(&mut self) {
        drop(Box::from_raw(self.future_ptr));
    }
}

impl<T> From<*mut dyn Future<Output=T>> for Task<T> {
    fn from(future: *mut dyn Future<Output=T>) -> Self {
        Self {
            future_ptr: future
        }
    }
}