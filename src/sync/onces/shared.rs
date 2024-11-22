use std::future::Future;
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::Ordering::{Acquire, Relaxed};

use crossbeam::utils::CachePadded;

use crate::sync::{AsyncOnce, OnceState};

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
/// ```rust
/// use orengine::sync::{AsyncOnce, Once};
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
}

impl AsyncOnce for Once {
    #[inline(always)]
    async fn call_once<Fut: Future<Output = ()>>(&self, f: Fut) -> Result<(), ()> {
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

    #[inline(always)]
    fn call_once_sync<F: FnOnce()>(&self, f: F) -> Result<(), ()> {
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

    #[inline(always)]
    fn state(&self) -> OnceState {
        OnceState::from(self.state.load(Acquire))
    }
}

impl Default for Once {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Sync for Once {}
unsafe impl Send for Once {}

/// ```rust
/// use orengine::sync::{Once, AsyncOnce, shared_scope};
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// async fn test() {
///     let once = Once::new();
///     let _ = check_send(once.call_once(async {})).await;
/// }
/// ```
#[allow(dead_code, reason = "It is used only in compile tests")]
fn test_compile_local() {}

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
        assert!(!once.is_completed());

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
        assert!(once.is_completed());
        assert_eq!(once.call_once(async {}).await, Err(()));
    }

    #[orengine_macros::test_shared]
    fn test_sync_shared_once() {
        let a = Arc::new(AtomicBool::new(false));
        let wg = Arc::new(WaitGroup::new());
        let once = Arc::new(Once::new());
        assert_eq!(once.state(), OnceState::NotCalled);
        assert!(!once.is_completed());

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
        assert!(once.is_completed());
        assert_eq!(once.call_once_sync(|| ()), Err(()));
    }
}
