use crate::sync::AsyncMutex;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

/// `WaitLockOfSubscribableMutex` implements [`Future`] that waits for a `lock`
/// with `subscription`.
///
/// For more details read the documentation of the trait [`AsyncSubscribableMutex`].
pub struct WaitLockOfSubscribableMutex<'mutex, T, Mutex>
where
    T: 'mutex + ?Sized,
    Mutex: AsyncSubscribableMutex<T> + ?Sized,
{
    mutex: &'mutex Mutex,
    was_called: bool,
    phantom_data: PhantomData<T>,
}

impl<'mutex, T, Mutex> WaitLockOfSubscribableMutex<'mutex, T, Mutex>
where
    T: 'mutex + ?Sized,
    Mutex: AsyncSubscribableMutex<T> + ?Sized,
{
    /// Creates a new `WaitLockOfSubscribableMutex`.
    pub fn new(mutex: &'mutex Mutex) -> Self {
        WaitLockOfSubscribableMutex { mutex, was_called: false, phantom_data: PhantomData }
    }
}

impl<'mutex, T, Mutex> Future for WaitLockOfSubscribableMutex<'mutex, T, Mutex>
where
    T: 'mutex + ?Sized,
    Mutex: AsyncSubscribableMutex<T> + ?Sized,
{
    type Output = Mutex::Guard<'mutex>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        if !this.was_called {
            this.was_called = true;
            this.mutex.low_level_subscribe(cx);

            Poll::Pending
        } else {
            Poll::Ready(unsafe { this.mutex.get_locked() })
        }
    }
}

/// `AsyncSubscribableMutex` is a trait that extends [`AsyncMutex`] by a
/// [`subscribe`](Self::subscribe) and [`low_level_subscribe`](Self::low_level_subscribe) methods.
///
/// For more details read [`subscribe`](Self::subscribe) and
/// [`low_level_subscribe`](Self::low_level_subscribe).
pub trait AsyncSubscribableMutex<T: ?Sized>: AsyncMutex<T> {
    /// Subscribing allow you to wait for the following [`unlock`](AsyncMutex::unlock) call.
    ///
    /// It means that one of the following calls [`unlock`](AsyncMutex::unlock) will wake
    /// current task up. It doesn't guarantee that [`unlock`](AsyncMutex::unlock) will be called
    /// or that the task will not wait if [`mutex`](AsyncMutex) is unlocked.
    ///
    /// This method a bit efficient than [`subscribe`](Self::subscribe).
    fn low_level_subscribe(&self, cx: &Context);

    /// Subscribing allow you to wait for the following [`unlock`](AsyncMutex::unlock) call.
    ///
    /// It means that one of the following calls [`unlock`](AsyncMutex::unlock) will wake
    /// current task up. It doesn't guarantee that [`unlock`](AsyncMutex::unlock) will be called
    /// or that the task will not wait if [`mutex`](AsyncMutex) is unlocked.
    ///
    /// __Code below is incorrect__:
    ///
    /// ```rust
    /// use orengine::sync::mutexes::AsyncSubscribableMutex;
    ///
    /// async fn do_with_locked_value<T, Mutex, F>(mutex: &Mutex, func: F)
    /// where
    ///     T: ?Sized,
    ///     Mutex: AsyncSubscribableMutex<T>,
    ///     F: FnOnce(&mut T)
    /// {
    ///     if let(Some(mut guard)) = mutex.try_lock() { // 1
    ///         func(&mut *guard);
    ///         return;
    ///     }
    ///
    ///     // 2 - here other thread can unlock the mutex
    ///
    ///     let mut guard = mutex.subscribe().await; // 3
    ///     func(&mut *guard);
    /// }
    /// ```
    ///
    /// Because between 1 and 3 [`mutex`](AsyncMutex) can be unlocked. Use
    /// [`lock`](AsyncMutex::lock) instead, because it is valid and more likely implemented via
    /// [`low_level_subscribe`](Self::low_level_subscribe) under the hood.
    ///
    /// [`subscribe`](Self::subscribe) is a bit more expensive than
    /// [`low_level_subscribe`](Self::low_level_subscribe).
    fn subscribe<'mutex>(&'mutex self) -> impl Future<Output=Self::Guard<'mutex>>
    where
        Self: 'mutex,
        T: 'mutex,
    {
        WaitLockOfSubscribableMutex::new(self)
    }
}