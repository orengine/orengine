use std::future::Future;
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::Ordering::{Acquire, Relaxed};

use crate::sync::{AsyncOnce, CallOnceResult, OnceState};
use crossbeam::utils::CachePadded;

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
///     START.call_once(async {
///         // some async code
///         println!("start");
///     }).await;
/// }
///
/// async fn print_msg_on_start() {
///     START.call_once_sync(|| {
///         println!("start");
///     });
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
    async fn call_once<Fut: Future<Output = ()>>(&self, f: Fut) -> CallOnceResult {
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
            CallOnceResult::Called
        } else {
            CallOnceResult::WasAlreadyCompleted
        }
    }

    #[inline(always)]
    fn call_once_sync<F: FnOnce()>(&self, f: F) -> CallOnceResult {
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

            CallOnceResult::Called
        } else {
            CallOnceResult::WasAlreadyCompleted
        }
    }

    #[inline(always)]
    fn state(&self) -> OnceState {
        #[cfg(debug_assertions)]
        {
            use crate::bug_message::BUG_MESSAGE;

            OnceState::try_from(self.state.load(Acquire)).expect(BUG_MESSAGE)
        }

        #[cfg(not(debug_assertions))]
        unsafe {
            OnceState::try_from(self.state.load(Acquire)).unwrap_unchecked()
        }
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
fn test_compile_shared_once() {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::sleep;
    use crate::sync::{AsyncWaitGroup, WaitGroup};
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

        wg.wait().await;
        assert!(once.is_completed());
        assert_eq!(
            once.call_once(async {}).await,
            CallOnceResult::WasAlreadyCompleted
        );
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

        wg.wait().await;
        assert!(once.is_completed());
        assert_eq!(
            once.call_once_sync(|| ()),
            CallOnceResult::WasAlreadyCompleted
        );
    }
}
