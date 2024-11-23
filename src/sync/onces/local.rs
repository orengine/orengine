use crate::sync::OnceState;
use crate::sync::{AsyncOnce, CallOnceResult};
use std::cell::Cell;
use std::future::Future;

/// `LocalOnce` is an asynchronous [`std::Once`](std::sync::Once).
///
/// # Usage
///
/// `LocalOnce` is used to call a function only once.
///
/// # The difference between `LocalOnce` and [`Once`](crate::sync::Once)
///
/// The `LocalOnce` works with `local tasks`.
///
/// Read [`Executor`](crate::Executor) for more details.
///
/// # Example
///
/// ```rust
/// use orengine::sync::{AsyncOnce, LocalOnce};
///
/// static START: LocalOnce = LocalOnce::new();
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
pub struct LocalOnce {
    state: Cell<OnceState>,
    // impl !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

impl LocalOnce {
    /// Creates a new `LocalOnce`.
    pub const fn new() -> Self {
        Self {
            state: Cell::new(OnceState::NotCalled),
            no_send_marker: std::marker::PhantomData,
        }
    }

    /// Non-Send future that implements [`AsyncOnce::call_once`].
    #[allow(clippy::future_not_send, reason = "It is `local`")]
    async fn call_once_no_send<Fut: Future<Output = ()>>(
        &self,
        f: Fut,
        _: std::marker::PhantomData<*const ()>,
    ) -> CallOnceResult {
        if self.is_completed() {
            return CallOnceResult::WasAlreadyCompleted;
        }

        self.state.replace(OnceState::Called);
        f.await;

        CallOnceResult::Called
    }
}

impl AsyncOnce for LocalOnce {
    #[inline(always)]
    #[allow(clippy::future_not_send, reason = "It is `local`")]
    async fn call_once<Fut: Future<Output = ()>>(&self, f: Fut) -> CallOnceResult {
        self.call_once_no_send(f, std::marker::PhantomData).await
    }

    #[inline(always)]
    fn call_once_sync<F: FnOnce()>(&self, f: F) -> CallOnceResult {
        if self.is_completed() {
            return CallOnceResult::WasAlreadyCompleted;
        }

        self.state.replace(OnceState::Called);
        f();

        CallOnceResult::Called
    }

    #[inline(always)]
    fn state(&self) -> OnceState {
        self.state.get()
    }
}

unsafe impl Sync for LocalOnce {}

impl Default for LocalOnce {
    fn default() -> Self {
        Self::new()
    }
}

/// ```compile_fail
/// use orengine::sync::{LocalOnce, AsyncOnce, shared_scope};
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// async fn test() {
///     let once = LocalOnce::new();
///     let _ = check_send(once.call_once(async {})).await;
/// }
/// ```
#[allow(dead_code, reason = "It is used only in compile tests")]
fn test_compile_local_once() {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;

    #[orengine::test::test_local]
    fn test_local_once() {
        let once = LocalOnce::new();
        assert_eq!(once.state(), OnceState::NotCalled);
        assert!(!once.is_completed());

        assert_eq!(once.call_once_sync(|| ()), CallOnceResult::Called);
        assert_eq!(once.state(), OnceState::Called);

        assert!(once.is_completed());
        assert_eq!(
            once.call_once_sync(|| ()),
            CallOnceResult::WasAlreadyCompleted
        );
    }

    #[orengine::test::test_local]
    fn test_local_once_async() {
        let async_once = LocalOnce::new();
        assert_eq!(async_once.state(), OnceState::NotCalled);
        assert!(!async_once.is_completed());

        assert_eq!(async_once.call_once(async {}).await, CallOnceResult::Called);
        assert_eq!(async_once.state(), OnceState::Called);

        assert!(async_once.is_completed());
        assert_eq!(
            async_once.call_once(async {}).await,
            CallOnceResult::WasAlreadyCompleted
        );
    }
}
