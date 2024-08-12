use std::future::Future;

#[derive(Copy, Clone)]
pub struct Task<T = ()> {
    pub(crate) future_ptr: *mut dyn Future<Output=T>,
}

impl<T> Task<T> {
    pub fn from_future<F: Future<Output=T> + 'static>(future: F) -> Self {
        Self {
            // TODO should we reuse memory?
            future_ptr: Box::leak(Box::new(future))
        }
    }

    pub unsafe fn drop_future(&mut self) {
        // TODO if we reuse, change it
        drop(Box::from_raw(self.future_ptr));
    }
}