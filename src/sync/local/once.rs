use std::cell::Cell;
use std::future::Future;

/// `OnceState` is used to indicate whether the `Once` has been called or not.
///
/// # Variants
///
/// * `NotCalled` - The `Once` has not been called.
/// * `Called` - The `Once` has been called.
#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum OnceState {
    NotCalled = 0,
    Called = 1,
}

impl OnceState {
    /// Returns the `OnceState` as an `isize`.
    pub const fn not_called() -> isize {
        0
    }

    /// Returns the `OnceState` as an `isize`.
    pub const fn called() -> isize {
        1
    }
}

impl Into<isize> for OnceState {
    fn into(self) -> isize {
        self as isize
    }
}

impl From<isize> for OnceState {
    fn from(state: isize) -> Self {
        match state {
            0 => OnceState::NotCalled,
            1 => OnceState::Called,
            _ => panic!("Invalid once state. It can be 0 (NotCalled) or 1 (Called)"),
        }
    }
}

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
/// use orengine::sync::LocalOnce;
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
    pub const fn new() -> LocalOnce {
        LocalOnce {
            state: Cell::new(OnceState::NotCalled),
            no_send_marker: std::marker::PhantomData,
        }
    }

    /// Calls the future only once.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::sync::LocalOnce;
    ///
    /// static START: LocalOnce = LocalOnce::new();
    ///
    /// async fn async_print_msg_on_start() {
    ///     let was_called = START.call_once(async {
    ///         // some async code
    ///         println!("start");
    ///     }).await.is_ok();
    /// }
    /// ```
    #[inline(always)]
    pub async fn call_once<Fut: Future<Output=()>>(&self, f: Fut) -> Result<(), ()> {
        if self.is_called() {
            return Err(());
        }

        self.state.replace(OnceState::Called);
        f.await;
        Ok(())
    }

    /// Calls the function only once.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::sync::LocalOnce;
    ///
    /// static START: LocalOnce = LocalOnce::new();
    ///
    /// async fn print_msg_on_start() {
    ///     let was_called = START.call_once_sync(|| {
    ///         println!("start");
    ///     }).is_ok();
    /// }
    /// ```
    #[inline(always)]
    pub fn call_once_sync<F: FnOnce()>(&self, f: F) -> Result<(), ()> {
        if self.is_called() {
            return Err(());
        }

        self.state.replace(OnceState::Called);
        f();
        Ok(())
    }

    /// Returns the [`state`](OnceState) of the `LocalOnce`.
    #[inline(always)]
    pub fn state(&self) -> OnceState {
        self.state.get()
    }

    /// Returns whether the `LocalOnce` has been called or not.
    #[inline(always)]
    pub fn is_called(&self) -> bool {
        self.state.get() == OnceState::Called
    }
}

unsafe impl Sync for LocalOnce {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;

    #[orengine_macros::test_local]
    fn test_local_once() {
        let once = LocalOnce::new();
        assert_eq!(once.state(), OnceState::NotCalled);
        assert!(!once.is_called());

        assert_eq!(once.call_once_sync(|| ()), Ok(()));
        assert_eq!(once.state(), OnceState::Called);

        assert!(once.is_called());
        assert_eq!(once.call_once_sync(|| ()), Err(()));
    }

    #[orengine_macros::test_local]
    fn test_local_once_async() {
        let async_once = LocalOnce::new();
        assert_eq!(async_once.state(), OnceState::NotCalled);
        assert!(!async_once.is_called());

        assert_eq!(async_once.call_once(async {}).await, Ok(()));
        assert_eq!(async_once.state(), OnceState::Called);

        assert!(async_once.is_called());
        assert_eq!(async_once.call_once(async {}).await, Err(()));
    }
}
