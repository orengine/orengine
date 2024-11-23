use crate::sync::mutexes::AsyncSubscribableMutex;
use crate::sync::AsyncMutex;
use std::future::Future;

/// `AsyncCondVar` is a `condition variable` that allows tasks to wait until
/// notified by another task.
///
/// It is designed to be used in conjunction with a [`AsyncSubscribableMutex`] to provide
/// a way for tasks to wait for a specific condition to occur.
///
/// # Attention
///
/// Drop a lock before call [`notify_one`](AsyncCondVar::notify_one)
/// or [`notify_all`](AsyncCondVar::notify_all) to improve performance.
///
/// # Examples
///
/// Read the documentation of [`LocalCondVar`](crate::sync::LocalCondVar)
/// and [`CondVar`](crate::sync::CondVar) for examples.
/// ```
pub trait AsyncCondVar {
    type SubscribableMutex<T>: ?Sized + AsyncSubscribableMutex<T>
    where
        T: ?Sized;

    /// Wait for a notification.
    ///
    /// ```rust
    /// use orengine::sync::{LocalCondVar, LocalMutex, local_scope, AsyncMutex, AsyncCondVar};
    /// use orengine::sleep;
    /// use std::time::Duration;
    ///
    /// # async fn test() {
    /// let cvar = LocalCondVar::new();
    /// let is_ready = LocalMutex::new(false);
    ///
    /// local_scope(|scope| async {
    ///     scope.spawn(async {
    ///         sleep(Duration::from_secs(1)).await;
    ///         let mut lock = is_ready.lock().await;
    ///         *lock = true;
    ///         drop(lock);
    ///         cvar.notify_one();
    ///     });
    ///
    ///     let mut lock = is_ready.lock().await;
    ///     while !*lock {
    ///         lock = cvar.wait(lock).await; // wait 1 second
    ///     }
    /// }).await;
    /// # }
    fn wait<'mutex, T>(
        &self,
        guard: <Self::SubscribableMutex<T> as AsyncMutex<T>>::Guard<'mutex>,
    ) -> impl Future<Output = <Self::SubscribableMutex<T> as AsyncMutex<T>>::Guard<'mutex>>
    where
        T: ?Sized + 'mutex;

    /// Notifies one waiting task.
    ///
    /// # Attention
    ///
    /// Drop a lock before call [`notify_one`](Self::notify_one).
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::sync::{AsyncSubscribableMutex, AsyncCondVar};
    ///
    /// async fn inc_counter_and_notify_one<'mutex, Mutex, CondVar>(counter: &Mutex, cvar: &CondVar)
    /// where
    ///     Mutex: AsyncSubscribableMutex<i32>,
    ///     CondVar: AsyncCondVar
    /// {
    ///     let mut lock = counter.lock().await;
    ///     *lock += 1;
    ///     drop(lock);
    ///     cvar.notify_one();
    /// }
    /// ```
    fn notify_one(&self);

    /// Notifies all waiting tasks.
    ///
    /// # Attention
    ///
    /// Drop a lock before call [`notify_all`](Self::notify_all).
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::sync::{AsyncSubscribableMutex, AsyncCondVar};
    ///
    /// async fn inc_counter_and_notify_all<'mutex, Mutex, CondVar>(counter: &Mutex, cvar: &CondVar)
    /// where
    ///     Mutex: AsyncSubscribableMutex<i32>,
    ///     CondVar: AsyncCondVar
    /// {
    ///     let mut lock = counter.lock().await;
    ///     *lock += 1;
    ///     drop(lock);
    ///     cvar.notify_all();
    /// }
    /// ```
    fn notify_all(&self);
}
