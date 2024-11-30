use crate::utils::{SpinLock, SpinLockGuard};

/// `NeverWaitLock` is a wrapper around [`SpinLock`] that never waits for the lock.
pub(crate) struct NeverWaitLock<T: ?Sized> {
    inner: SpinLock<T>,
}

impl<T: ?Sized> NeverWaitLock<T> {
    /// Creates a new `NeverWaitLock` with the given value.
    pub(crate) const fn new(value: T) -> Self
    where
        T: Sized,
    {
        Self {
            inner: SpinLock::new(value),
        }
    }

    /// if `NeverWaitLock` is unlocked returns [`SpinLockGuard`], otherwise returns [`None`].
    pub(crate) fn try_lock(&self) -> Option<SpinLockGuard<T>> {
        self.inner.try_lock()
    }
}
