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

impl Future for Asyncify<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        if !this.was_called {
            this.was_called = true;
            let ptr = unsafe {
                #[allow(
                    clippy::transmute_ptr_to_ptr,
                    reason = "'future is equal to 'static in this code"
                )]
                mem::transmute::<*mut dyn Fn(), *mut dyn Fn()>(this.f)
            };
            unsafe {
                local_executor().push_fn_to_thread_pool(ptr);
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
/// ```rust
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

    #[orengine::test::test_local]
    fn test_asyncify() {
        let start = time::Instant::now();
        let was_changed = Arc::new(std::sync::Mutex::new(false));
        let cond_var = Arc::new(std::sync::Condvar::new());
        let was_changed_clone = was_changed.clone();
        let cond_var_clone = cond_var.clone();

        local_executor().exec_local_future(async move {
            asyncify!(|| {
                thread::sleep(Duration::from_millis(1));
                assert!(start.elapsed() >= Duration::from_millis(1));
                *was_changed_clone.lock().unwrap() = true;
                cond_var_clone.notify_one();
            })
            .await;
        });

        let mut guard = was_changed.lock().unwrap();
        guard = cond_var.wait(guard).unwrap();

        assert!(*guard); // this executor was blocked, and
                         // if assertion passes, other thread (from the thread pool) processed the list.
        drop(guard);
    }
}
