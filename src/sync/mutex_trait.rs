use std::future::Future;
use std::ops::{Deref, DerefMut};

// TODO docs
pub trait AsyncMutexGuard<'mutex, T: ?Sized>: Deref<Target=T> + DerefMut {
    // TODO docs
    type Mutex: AsyncMutex<T> + ?Sized;

    /// Returns a reference to the original [`Mutex`].
    fn mutex(&self) -> &Self::Mutex;

    /// Returns a reference to the original [`NaiveMutex`].
    ///
    /// The `mutex` will never be unlocked.
    ///
    /// # Safety
    ///
    /// The `mutex` is unlocked by calling [`AsyncMutexGuard::unlock`] later.
    unsafe fn leak(self) -> &'mutex Self::Mutex;
}

/// ```compile_fail
/// use orengine::sync::{NaiveMutex, AsyncMutex};
/// use orengine::yield_now;
///
/// struct NonSend {
///     value: i32,
///     // impl !Send
///     no_send_marker: std::marker::PhantomData<*const ()>,
/// }
///
/// async fn test() {
///     let mutex = NaiveMutex::new(NonSend {
///         value: 0,
///         no_send_marker: std::marker::PhantomData,
///     });
///
///     let guard = mutex.lock().await;
///     yield_now().await;
///     assert_eq!(guard.value, 0);
///     drop(guard);
/// }
/// ```
///
/// ```rust
/// use orengine::sync::{NaiveMutex, AsyncMutex};
/// use orengine::yield_now;
///
/// // impl Send
/// struct NonSend {
///     value: i32,
/// }
///
/// async fn test() {
///     let mutex = NaiveMutex::new(NonSend {
///         value: 0,
///     });
///
///     let guard = mutex.lock().await;
///     yield_now().await;
///     assert_eq!(guard.value, 0);
///     drop(guard);
/// }
/// ```
#[allow(dead_code, reason = "It is used only in compile tests")]
fn test_compile() {}

// TODO docs
pub trait AsyncMutex<T: ?Sized> {
    // TODO docs
    type Guard<'mutex>: AsyncMutexGuard<'mutex, T, Mutex=Self>
    where
        Self: 'mutex;

    /// Returns whether the `mutex` is locked.
    fn is_locked(&self) -> bool;

    /// Returns a [`Future`] that resolves to [`AsyncMutexGuard`]
    /// that allows access to the inner value.
    ///
    /// It blocks the current task if the `mutex` is locked.
    fn lock(&self) -> impl Future<Output=Self::Guard<'_>>;

    /// If the `mutex` is not locked, returns [`AsyncMutexGuard`], otherwise returns [`None`].
    fn try_lock(&self) -> Option<Self::Guard<'_>>;

    /// Returns a reference to the underlying data. It is safe because it uses `&mut self`.
    fn get_mut(&mut self) -> &mut T;

    /// Unlocks the `mutex`.
    ///
    /// # Safety
    ///
    /// - The `mutex` must be locked.
    ///
    /// - No other tasks has an ownership of this `mutex`.
    unsafe fn unlock(&self);

    /// Returns a reference to the inner value.
    ///
    /// # Safety
    ///
    /// - The mutex must be locked.
    ///
    /// - And only current task has an ownership of this `lock`.
    unsafe fn get_locked(&self) -> &T;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    // TODO rewrite all `orengine_macros::test_shared` and `orengine_macros::test_local` to crate::test::test_local
    use crate::test::{test_local, test_shared};

    struct NonSend {
        value: i32,
        // impl !Send
        no_send_marker: std::marker::PhantomData<*const ()>,
    }

    #[test_local]
    fn test_local_async_mutex() {
        let mutex = crate::sync::LocalMutex::new(NonSend {
            value: 0,
            no_send_marker: std::marker::PhantomData,
        });

        let mut guard = mutex.lock().await;
        assert_eq!(guard.value, 0);
        assert!(mutex.is_locked());
        assert!(mutex.try_lock().is_none());
        guard.value += 1;
        drop(guard);

        let mut guard = mutex.try_lock().unwrap();
        assert_eq!(guard.value, 1);
        assert!(mutex.is_locked());
        guard.value += 1;
        drop(guard);

        assert!(!mutex.is_locked());
    }

    #[test_shared]
    fn test_shared_async_mutex() {
        let mutex = crate::sync::NaiveMutex::new(0);

        let mut guard = mutex.lock().await;
        assert_eq!(*guard, 0);
        assert!(mutex.is_locked());
        assert!(mutex.try_lock().is_none());
        *guard += 1;
        drop(guard);

        let mut guard = mutex.try_lock().unwrap();
        assert_eq!(*guard, 1);
        assert!(mutex.is_locked());
        *guard += 1;
        drop(guard);

        assert!(!mutex.is_locked());
    }
}