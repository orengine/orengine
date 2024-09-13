use std::future::Future;
use std::mem;
use std::pin::Pin;
use std::task::Poll;
use crate::local_executor;

pub struct Asyncify<'future> {
    f: &'future mut dyn Fn(),
    was_called: bool
}

impl<'future> Asyncify<'future> {
    pub fn new(f: &'future mut dyn Fn()) -> Self {
        Self {
            f,
            was_called: false
        }
    }
}

impl<'future> Future for Asyncify<'future> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        if !this.was_called {
            this.was_called = true;
            unsafe {
                local_executor().push_fn_to_thread_pool(mem::transmute(this.f as *mut dyn Fn()));
            };

            return Poll::Pending;
        }

        Poll::Ready(())
    }
}


#[macro_export]
macro_rules! asyncify {
    ($f:expr) => {
        $crate::runtime::asyncify::Asyncify::new(&mut $f)
    }
}

#[cfg(test)]
mod tests {
    use std::{thread, time};
    use std::sync::Arc;
    use std::time::Duration;
    use crate::local_executor;

    #[orengine_macros::test]
    fn test_asyncify() {
        let start = time::Instant::now();
        let list = Arc::new(std::sync::Mutex::new(vec![]));
        let list_clone = list.clone();

        local_executor().exec_future(async move {
            asyncify!(|| {
                list_clone.lock().unwrap().push(1);
                thread::sleep(Duration::from_millis(1));
                assert!(start.elapsed() >= Duration::from_millis(1));
                list_clone.lock().unwrap().push(2);
            }).await;
        });

        assert_eq!(*list.lock().unwrap(), vec![]);

        thread::sleep(Duration::from_millis(2));
        assert_eq!(*list.lock().unwrap(), vec![1, 2]); // this executor was blocked for 2ms, and
        // if assertion passes, other thread (from the thread pool) processed the list.
    }
}