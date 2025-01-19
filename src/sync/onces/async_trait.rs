use crate::sync::OnceState;
use std::future::Future;

/// `CallOnceResult` is the result of [`call_once`](AsyncOnce::call_once) or
/// [`call_once_sync`](AsyncOnce::call_once_sync).
///
/// It is used to determine whether the [`future`](Future) or the function has been called or not.
#[derive(Debug, PartialEq, Eq)]
pub enum CallOnceResult {
    /// The [`future`](Future) or the function has been called.
    Called,
    /// The [`future`](Future) or the function has not been called because
    /// [`AsyncOnce::call_once`]
    /// or [`AsyncOnce::call_once_sync`] has been already called.
    WasAlreadyCompleted,
}

/// `AsyncOnce` is an asynchronous [`std::Once`](std::sync::Once).
///
/// # Usage
///
/// `AsyncOnce` is used to call a function or [`future`](Future) only once.
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
pub trait AsyncOnce {
    /// Calls the [`future`](Future) only once.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::sync::{LocalOnce, AsyncOnce};
    ///
    /// static START: LocalOnce = LocalOnce::new();
    ///
    /// async fn async_print_msg_on_start() {
    ///     START.call_once(async {
    ///         // some async code
    ///         println!("start");
    ///     }).await;
    /// }
    /// ```
    fn call_once<Fut: Future<Output=()>>(&self, f: Fut) -> impl Future<Output=CallOnceResult>;

    /// Calls the function only once.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::sync::{AsyncOnce, LocalOnce};
    ///
    /// static START: LocalOnce = LocalOnce::new();
    ///
    /// async fn print_msg_on_start() {
    ///     START.call_once_sync(|| {
    ///         println!("start");
    ///     });
    /// }
    /// ```
    fn call_once_sync<F: FnOnce()>(&self, f: F) -> CallOnceResult;

    /// Returns the [`state`](OnceState) of the `AsyncOnce`.
    fn state(&self) -> OnceState;

    /// Returns whether the `AsyncOnce` has been called or not.
    #[inline]
    fn is_completed(&self) -> bool {
        self.state() == OnceState::Called
    }
}
