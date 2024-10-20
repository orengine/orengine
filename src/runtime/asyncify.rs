use crate::local_executor;
use std::future::Future;
use std::mem;
use std::pin::Pin;
use std::task::Poll;

/// A function that must be executed in the blocking pool.
pub struct Asyncify<'future> {
    f: &'future mut dyn Fn(),
    was_called: bool,
}

impl<'future> Asyncify<'future> {
    /// Create a new [`Asyncify`] with the given function.
    pub fn new(f: &'future mut dyn Fn()) -> Self {
        Self {
            f,
            was_called: false,
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

/// Create a new [`Asyncify`] from the given function.
///
/// Use it to run a blocking function in async runtime.
///
/// # Example
///
/// ```no_run
/// use orengine::asyncify;
///
/// # fn blocking_code() {}
///
/// async fn foo() {
///     asyncify!(|| {
///         // call blocking code here
///         blocking_code();
///     }).await;
/// }
/// ```
#[macro_export]
macro_rules! asyncify {
    ($f:expr) => {
        $crate::runtime::asyncify::Asyncify::new(&mut $f)
    };
}

#[cfg(test)]
mod tests {
    use crate as orengine;
    use crate::local_executor;
    use std::sync::Arc;
    use std::time::Duration;
    use std::{thread, time};

    #[orengine_macros::test_local]
    fn test_asyncify() {
        let start = time::Instant::now();
        let list = Arc::new(std::sync::Mutex::new(vec![]));
        let cond_var = Arc::new(std::sync::Condvar::new());
        let list_clone = list.clone();
        let cond_var_clone = cond_var.clone();

        local_executor().exec_local_future(async move {
            asyncify!(|| {
                list_clone.lock().unwrap().push(1);
                thread::sleep(Duration::from_millis(1));
                assert!(start.elapsed() >= Duration::from_millis(1));
                list_clone.lock().unwrap().push(2);
                cond_var_clone.notify_one();
            })
            .await;
        });

        let mut guard = list.lock().unwrap();
        assert_eq!(*guard, vec![]);
        guard = cond_var.wait(guard).unwrap();

        assert_eq!(*guard, vec![1, 2]); // this executor was blocked, and
                                        // if assertion passes, other thread (from the thread pool) processed the list.
    }
}
