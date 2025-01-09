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
pub trait AsyncCondVar {
    type SubscribableMutex<T>: ?Sized + AsyncSubscribableMutex<T>
    where
        T: ?Sized;

    /// Wait for a notification.
    ///
    /// # Example
    ///
    /// ```no_run
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
    ///
    ///         let mut lock = is_ready.lock().await;
    ///         *lock = true;
    ///
    ///         drop(lock);
    ///
    ///         cvar.notify_one();
    ///     });
    ///
    ///     let mut lock = is_ready.lock().await;
    ///     while !*lock {
    ///         lock = cvar.wait(lock).await; // wait 1 second
    ///     }
    /// }).await;
    /// # }
    /// ```
    fn wait<'mutex, T>(
        &self,
        guard: <Self::SubscribableMutex<T> as AsyncMutex<T>>::Guard<'mutex>,
    ) -> impl Future<Output = <Self::SubscribableMutex<T> as AsyncMutex<T>>::Guard<'mutex>>
    where
        T: ?Sized + 'mutex;

    /// Blocks the current [`Task`] until the provided condition becomes false.
    ///
    /// `condition` is checked immediately; if not met (returns `true`), this
    /// will [`wait`](Self::wait) for the next notification then check again. This repeats
    /// until `condition` returns `false`, in which case this function returns.
    ///
    /// This function will atomically unlock the mutex specified (represented by
    /// `guard`) and block the current [`Task`]. This means that any calls
    /// to [`notify_one`] or [`notify_all`] which happen logically after the
    /// mutex is unlocked are candidates to wake this [`Task`] up. When this
    /// function call returns, the lock specified will have been re-acquired.
    ///
    /// [`notify_one`]: Self::notify_one
    /// [`notify_all`]: Self::notify_all
    /// [`Task`]: crate::runtime::Task
    ///
    /// # Example
    ///
    /// ```no_run
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
    ///
    ///         let mut lock = is_ready.lock().await;
    ///         *lock = true;
    ///
    ///         drop(lock);
    ///
    ///         cvar.notify_one();
    ///     });
    ///
    ///     let mut is_ready_lock = cvar.wait_while(
    ///         is_ready.lock().await,
    ///         |lock| !*lock
    ///     ).await; // wait 1 second
    /// }).await;
    /// # }
    /// ```
    async fn wait_while<'mutex, T>(
        &self,
        guard: <Self::SubscribableMutex<T> as AsyncMutex<T>>::Guard<'mutex>,
        predicate: impl Fn(&mut T) -> bool,
    ) -> <Self::SubscribableMutex<T> as AsyncMutex<T>>::Guard<'mutex>
    where
        T: ?Sized + 'mutex,
    {
        let mut guard = guard;
        while predicate(&mut guard) {
            guard = self.wait(guard).await;
        }

        guard
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::sleep;
    use crate::sync::{local_scope, LocalCondVar, LocalMutex};
    use std::time::Duration;

    #[orengine::test::test_local]
    fn test_cond_var_wait_while() {
        let cvar = LocalCondVar::new();
        let is_ready = LocalMutex::new(false);
        local_scope(|scope| async {
            scope.spawn(async {
                sleep(Duration::from_secs(1)).await;

                let mut lock = is_ready.lock().await;
                *lock = true;

                drop(lock);

                cvar.notify_one();
            });

            let is_ready_lock = cvar.wait_while(is_ready.lock().await, |lock| !*lock).await; // wait 1 second

            assert!(*is_ready_lock);
        })
        .await;
    }
}
