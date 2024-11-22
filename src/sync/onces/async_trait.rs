use crate::sync::OnceState;
use std::future::Future;

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
    ///     let was_called = START.call_once(async {
    ///         // some async code
    ///         println!("start");
    ///     }).await.is_ok();
    /// }
    /// ```
    fn call_once<Fut: Future<Output = ()>>(&self, f: Fut) -> impl Future<Output = Result<(), ()>>;

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
    ///     let was_called = START.call_once_sync(|| {
    ///         println!("start");
    ///     }).is_ok();
    /// }
    /// ```
    fn call_once_sync<F: FnOnce()>(&self, f: F) -> Result<(), ()>;

    /// Returns the [`state`](OnceState) of the `AsyncOnce`.
    fn state(&self) -> OnceState;

    /// Returns whether the `AsyncOnce` has been called or not.
    #[inline(always)]
    fn is_completed(&self) -> bool {
        self.state() == OnceState::Called
    }
}
