use std::future::Future;
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::Ordering::{Acquire, Relaxed};

use crossbeam::utils::CachePadded;

use crate::sync::local::once::OnceState;

/// `Once` is an asynchronous [`std::Once`](std::sync::Once).
///
/// # Usage
///
/// `Once` is used to call a function only once.
///
/// # The difference between `Once` and [`LocalOnce`](crate::sync::LocalOnce)
///
/// The `Once` works with `shared tasks` and can be shared between threads.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// # Example
///
/// ```no_run
/// use orengine::sync::Once;
///
/// static START: Once = Once::new();
///
/// async fn async_print_msg_on_start() {
///     let was_called = START.call_once(async {
///         // some async code
///         println!("start");
///     }).await.is_ok();
/// }
///
/// async fn print_msg_on_start() {
///     let was_called = START.call_once_sync(|| {
///         println!("start");
///     }).is_ok();
/// }
/// ```
pub struct Once {
    state: CachePadded<AtomicIsize>,
}

impl Once {
    /// Creates a new `Once`.
    pub const fn new() -> Once {
        Once {
            state: CachePadded::new(AtomicIsize::new(OnceState::not_called())),
        }
    }

    /// Calls the future only once.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::sync::Once;
    ///
    /// static START: Once = Once::new();
    ///
    /// async fn async_print_msg_on_start() {
    ///     let was_called = START.call_once(async {
    ///         // some async code
    ///         println!("start");
    ///     }).await.is_ok();
    /// }
    /// ```
    #[inline(always)]
    pub async fn call_once<Fut: Future<Output = ()>>(&self, f: Fut) -> Result<(), ()> {
        if self
            .state
            .compare_exchange(
                OnceState::NotCalled.into(),
                OnceState::Called.into(),
                Acquire,
                Relaxed,
            )
            .is_ok()
        {
            f.await;
            Ok(())
        } else {
            Err(())
        }
    }

    /// Calls the function only once.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::sync::Once;
    ///
    /// static START: Once = Once::new();
    ///
    /// async fn print_msg_on_start() {
    ///     let was_called = START.call_once_sync(|| {
    ///         println!("start");
    ///     }).is_ok();
    /// }
    /// ```
    #[inline(always)]
    pub fn call_once_sync<F: FnOnce()>(&self, f: F) -> Result<(), ()> {
        if self
            .state
            .compare_exchange(
                OnceState::NotCalled.into(),
                OnceState::Called.into(),
                Acquire,
                Relaxed,
            )
            .is_ok()
        {
            f();
            Ok(())
        } else {
            Err(())
        }
    }

    /// Returns the [`state`](OnceState) of the `Once`.
    #[inline(always)]
    pub fn state(&self) -> OnceState {
        OnceState::from(self.state.load(Acquire))
    }

    /// Returns whether the `Once` has been called or not.
    #[inline(always)]
    pub fn was_called(&self) -> bool {
        self.state() == OnceState::Called
    }
}

unsafe impl Sync for Once {}
unsafe impl Send for Once {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::sleep;
    use crate::sync::WaitGroup;
    use crate::test::sched_future_to_another_thread;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering::SeqCst;
    use std::sync::Arc;
    use std::time::Duration;

    #[orengine_macros::test_shared]
    fn test_async_shared_once() {
        let a = Arc::new(AtomicBool::new(false));
        let wg = Arc::new(WaitGroup::new());
        let once = Arc::new(Once::new());
        assert_eq!(once.state(), OnceState::NotCalled);
        assert!(!once.was_called());

        for _ in 0..10 {
            let a = a.clone();
            let wg = wg.clone();
            wg.add(1);
            let once = once.clone();
            sched_future_to_another_thread(async move {
                let _ = once
                    .call_once(async move {
                        sleep(Duration::from_millis(1)).await;
                        assert!(!a.load(SeqCst));
                        a.store(true, SeqCst);
                    })
                    .await;
                wg.done();
            });
        }

        let _ = wg.wait().await;
        assert!(once.was_called());
        assert_eq!(once.call_once(async {}).await, Err(()));
    }

    #[orengine_macros::test_shared]
    fn test_sync_shared_once() {
        let a = Arc::new(AtomicBool::new(false));
        let wg = Arc::new(WaitGroup::new());
        let once = Arc::new(Once::new());
        assert_eq!(once.state(), OnceState::NotCalled);
        assert!(!once.was_called());

        for _ in 0..10 {
            let a = a.clone();
            let wg = wg.clone();
            wg.add(1);
            let once = once.clone();
            sched_future_to_another_thread(async move {
                let _ = once.call_once_sync(|| {
                    assert!(!a.load(SeqCst));
                    a.store(true, SeqCst);
                });
                wg.done();
            });
        }

        let _ = wg.wait().await;
        assert!(once.was_called());
        assert_eq!(once.call_once_sync(|| ()), Err(()));
    }
}
