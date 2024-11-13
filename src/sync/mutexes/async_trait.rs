use std::future::Future;
use std::ops::{Deref, DerefMut};

/// An RAII implementation of a "scoped lock" of a `mutex`. When this structure is
/// dropped (falls out of scope), the `mutex` will be unlocked.
///
/// The data protected by the `mutex` can be accessed through this guard via its
/// [`Deref`](Deref) and [`DerefMut`] implementations.
///
/// This structure is created by the [`lock`](AsyncMutex::lock)
/// and [`try_lock`](AsyncMutex::try_lock).
pub trait AsyncMutexGuard<'mutex, T: ?Sized>: Deref<Target=T> + DerefMut {
    /// The type of the `mutex` associated with this `guard`.
    ///
    /// It implements [`AsyncMutex`].
    type Mutex: AsyncMutex<T> + ?Sized;

    /// Returns a reference to the original [`Mutex`].
    fn mutex(&self) -> &Self::Mutex;

    /// Returns a reference to the original [`Mutex`](Self::Mutex).
    ///
    /// The `mutex` will never be unlocked.
    ///
    /// # Safety
    ///
    /// The `mutex` is unlocked by calling [`AsyncMutex::unlock`] later.
    unsafe fn leak(self) -> &'mutex Self::Mutex;
}

/// A mutual exclusion primitive useful for protecting shared (between tasks) data.
///
/// If the data is shared in a single thread (read about `local` tasks in
/// [`Executor`](crate::Executor)), use [`LocalMutex`](crate::sync::LocalMutex).
///
/// Else use [`Mutex`](crate::sync::Mutex) or [`NaiveMutex`](crate::sync::NaiveMutex).
///
/// `AsyncMutex` will block tasks waiting for the lock to become available.
/// Each mutex has a type parameter which represents the data that it is protecting.
/// The data can be accessed through the RAII guards returned from [`lock`](AsyncMutex::lock)
/// and [`try_lock`](AsyncMutex::try_lock), which guarantees that the data is only ever
/// accessed when the `mutex` is locked, or with an unsafe method [`get_locked`](Mutex::get_locked).
///
/// # Example
///
/// ```rust
/// use std::collections::HashMap;
/// use orengine::sync::AsyncMutex;
///
/// # async fn write_to_the_dump_file(key: usize, value: usize) {}
///
/// async fn dump_storage<Mu: AsyncMutex<HashMap<usize, usize>>>(storage: &Mu) {
///     let mut guard = storage.lock().await;
///
///     for (key, value) in guard.iter() {
///         write_to_the_dump_file(*key, *value).await;
///     }
///
///     // lock is released when `guard` goes out of scope
/// }
/// ```
pub trait AsyncMutex<T: ?Sized> {
    /// The type of the `guard` that is returned from the [`lock`](AsyncMutex::lock) method.
    ///
    /// An RAII implementation of a "scoped lock" of a `mutex`. When this structure is
    /// dropped (falls out of scope), the `mutex` will be unlocked.
    ///
    /// The data protected by the `mutex` can be accessed through this guard via its
    /// [`Deref`](Deref) and [`DerefMut`] implementations.
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

    /// If the `mutex` is not locked, locks it and returns [`AsyncMutexGuard`],
    /// otherwise returns [`None`].
    fn try_lock(&self) -> Option<Self::Guard<'_>>;

    /// Returns a reference to the underlying data. It is safe because it uses `&mut self`.
    fn get_mut(&mut self) -> &mut T;

    /// Unlocks the `mutex`.
    ///
    /// # Safety
    ///
    /// - The `mutex` must be locked.
    ///
    /// - No other tasks has an ownership of this `lock`.
    unsafe fn unlock(&self);

    /// Returns a reference to the inner value.
    ///
    /// # Safety
    ///
    /// - The `mutex` must be locked.
    ///
    /// - Only current task has an ownership of this `lock`.
    unsafe fn get_locked(&self) -> &mut T;
}

/// ```compile_fail
/// use orengine::sync::{NaiveMutex, AsyncMutex};
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
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
///     let guard = check_send(mutex.lock()).await;
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
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// // impl Send
/// struct CanSend {
///     value: i32,
/// }
///
/// async fn test() {
///     let mutex = NaiveMutex::new(CanSend {
///         value: 0,
///     });
///
///     let guard = check_send(mutex.lock()).await;
///     yield_now().await;
///     assert_eq!(guard.value, 0);
///     drop(guard);
/// }
/// ```
#[allow(dead_code, reason = "It is used only in compile tests")]
fn test_compile_naive() {}

/// ```compile_fail
/// use orengine::sync::{Mutex, AsyncMutex};
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// struct NonSend {
///     value: i32,
///     // impl !Send
///     no_send_marker: std::marker::PhantomData<*const ()>,
/// }
///
/// async fn test() {
///     let mutex = Mutex::new(NonSend {
///         value: 0,
///         no_send_marker: std::marker::PhantomData,
///     });
///
///     let guard = check_send(mutex.lock()).await;
///     yield_now().await;
///     assert_eq!(guard.value, 0);
///     drop(guard);
/// }
/// ```
///
/// ```rust
/// use orengine::sync::{Mutex, AsyncMutex};
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// // impl Send
/// struct CanSend {
///     value: i32,
/// }
///
/// async fn test() {
///     let mutex = Mutex::new(CanSend {
///         value: 0,
///     });
///
///     let guard = check_send(mutex.lock()).await;
///     yield_now().await;
///     assert_eq!(guard.value, 0);
///     drop(guard);
/// }
/// ```
#[allow(dead_code, reason = "It is used only in compile tests")]
fn test_compile_smart() {}

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