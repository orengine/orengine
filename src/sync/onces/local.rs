use crate::sync::AsyncOnce;
use crate::sync::OnceState;
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

    async fn call_once_no_send<Fut: Future<Output = ()>>(
        &self,
        f: Fut,
        _: std::marker::PhantomData<*const ()>,
    ) -> Result<(), ()> {
        if self.is_completed() {
            return Err(());
        }

        self.state.replace(OnceState::Called);
        f.await;

        Ok(())
    }
}

impl AsyncOnce for LocalOnce {
    #[inline(always)]
    #[allow(clippy::future_not_send, reason = "It is `local`")]
    async fn call_once<Fut: Future<Output = ()>>(&self, f: Fut) -> Result<(), ()> {
        self.call_once_no_send(f, std::marker::PhantomData).await
    }

    #[inline(always)]
    fn call_once_sync<F: FnOnce()>(&self, f: F) -> Result<(), ()> {
        if self.is_completed() {
            return Err(());
        }

        self.state.replace(OnceState::Called);
        f();

        Ok(())
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
fn test_compile_local() {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;

    #[orengine_macros::test_local]
    fn test_local_once() {
        let once = LocalOnce::new();
        assert_eq!(once.state(), OnceState::NotCalled);
        assert!(!once.is_completed());

        assert_eq!(once.call_once_sync(|| ()), Ok(()));
        assert_eq!(once.state(), OnceState::Called);

        assert!(once.is_completed());
        assert_eq!(once.call_once_sync(|| ()), Err(()));
    }

    #[orengine_macros::test_local]
    fn test_local_once_async() {
        let async_once = LocalOnce::new();
        assert_eq!(async_once.state(), OnceState::NotCalled);
        assert!(!async_once.is_completed());

        assert_eq!(async_once.call_once(async {}).await, Ok(()));
        assert_eq!(async_once.state(), OnceState::Called);

        assert!(async_once.is_completed());
        assert_eq!(async_once.call_once(async {}).await, Err(()));
    }
}
