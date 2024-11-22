use crate::sync::LockStatus;
use std::future::Future;
use std::ops::{Deref, DerefMut};

/// RAII structure used to release the shared read access of a `read-lock` when
/// dropped.
///
/// The protected data can be accessed through this guard via its [`Deref`] implementation.
///
/// This structure is created by the [`AsyncRWLock::read`], [`AsyncRWLock::try_read`]
/// and [`AsyncRWLock::read_unlock`] methods.
pub trait AsyncReadLockGuard<'rw_lock, T: ?Sized + 'rw_lock>: Deref<Target = T> {
    /// The type of `rear-write lock` associated with this `guard`.
    ///
    /// It implements [`AsyncRWLock`].
    type RWLock: AsyncRWLock<T> + ?Sized;

    /// Returns a reference to the original [`AsyncRWLock`].
    fn rw_lock(&self) -> &'rw_lock Self::RWLock;

    /// Returns a reference to the original [`AsyncRWLock`].
    ///
    /// The read portion of this lock will not be released.
    ///
    /// # Safety
    ///
    /// The [`AsyncRWLock`] is read unlocked by calling [`AsyncRWLock::read_unlock`] later.
    unsafe fn leak(self) -> &'rw_lock Self::RWLock;
}

/// RAII structure used to release the unique write access of a `write-lock` when
/// dropped.
///
/// The protected data can be accessed through this guard via its [`Deref`] and
/// [`DerefMut`] implementations.
///
/// This structure is created by the [`AsyncRWLock::write`], [`AsyncRWLock::try_write`] and
/// [`AsyncRWLock::write_unlock`] methods.
pub trait AsyncWriteLockGuard<'rw_lock, T: ?Sized + 'rw_lock>:
    Deref<Target = T> + DerefMut
{
    /// The type of `rear-write lock` associated with this `guard`.
    ///
    /// It implements [`AsyncRWLock`].
    type RWLock: AsyncRWLock<T> + ?Sized;

    /// Returns a reference to the original [`AsyncRWLock`].
    fn rw_lock(&self) -> &'rw_lock Self::RWLock;

    /// Returns a reference to the original [`AsyncRWLock`].
    ///
    /// The read portion of this lock will not be released.
    ///
    /// # Safety
    ///
    /// The [`AsyncRWLock`] is read unlocked by calling [`AsyncRWLock::read_unlock`] later.
    unsafe fn leak(self) -> &'rw_lock Self::RWLock;
}

/// An asynchronous version of a [`reader-writer lock`](std::sync::RwLock).
///
/// This type of lock allows a number of readers or at most one writer at any
/// point in time. The write portion of this lock typically allows modification
/// of the underlying data (exclusive access) and the read portion of this lock
/// typically allows for read-only access (shared access).
///
/// In comparison, a [`Mutex`](crate::sync::Mutex)
/// does not distinguish between readers or writers
/// that acquire the lock, therefore blocking any tasks waiting for the lock to
/// become available. An `RWLock` will allow any number of readers to acquire the
/// lock as long as a writer is not holding the lock.
///
/// The type parameter `T` represents the data that this lock protects. It is
/// required that `T` satisfies [`Sync`] to allow concurrent access through readers. The RAII guards
/// returned from the locking methods implement [`Deref`] (and [`DerefMut`]
/// for the `write` methods) to allow access to the content of the lock.
///
/// # Example
///
/// ```rust
/// use std::collections::HashMap;
/// use orengine::sync::AsyncRWLock;
///
/// # async fn write_to_the_dump_file(key: usize, value: usize) {}
///
/// async fn dump_storage<S: AsyncRWLock<HashMap<usize, usize>>>(storage: &S) {
///     let mut read_guard = storage.read().await;
///     
///     for (key, value) in read_guard.iter() {
///         write_to_the_dump_file(*key, *value).await;
///     }
///
///     // read lock is released when `guard` goes out of scope
/// }
/// ```
pub trait AsyncRWLock<T: ?Sized> {
    /// `Read access lock guard` of `AsyncRWLock`. Read more in [`AsyncReadLockGuard`].
    type ReadLockGuard<'rw_lock>: AsyncReadLockGuard<'rw_lock, T>
    where
        T: 'rw_lock,
        Self: 'rw_lock;
    /// `Write access lock guard` of `AsyncRWLock`. Read more in [`AsyncWriteLockGuard`].
    type WriteLockGuard<'rw_lock>: AsyncWriteLockGuard<'rw_lock, T>
    where
        T: 'rw_lock,
        Self: 'rw_lock;

    /// Returns the current [`status`](LockStatus) of the lock.
    fn get_lock_status(&self) -> LockStatus;

    /// Returns associated [`ReadLockGuard`](Self::ReadLockGuard) that allows mutable access
    /// to the inner value.
    ///
    /// It will block the current task if the lock is already acquired for writing or reading.
    fn write<'rw_lock>(&'rw_lock self) -> impl Future<Output = Self::WriteLockGuard<'rw_lock>>
    where
        T: 'rw_lock;

    /// Returns associated [`ReadLockGuard`](Self::ReadLockGuard) that allows
    /// shared access to the inner value.
    ///
    /// It will block the current task if the lock is already acquired for writing.
    fn read<'rw_lock>(&'rw_lock self) -> impl Future<Output = Self::ReadLockGuard<'rw_lock>>
    where
        T: 'rw_lock;

    /// If the lock is not acquired for writing or reading, returns associated
    /// [`WriteLockGuard`](Self::WriteLockGuard) that allows mutable access to the inner value,
    /// otherwise returns [`None`].
    fn try_write(&self) -> Option<Self::WriteLockGuard<'_>>;

    /// If the lock is not acquired for writing, returns associated
    /// [`ReadLockGuard`](Self::ReadLockGuard) that allows shared access to the inner value,
    /// otherwise returns [`None`].
    fn try_read(&self) -> Option<Self::ReadLockGuard<'_>>;

    /// Returns a mutable reference to the inner value. It is safe because it uses `&mut self`.
    fn get_mut(&mut self) -> &mut T;

    /// Releases the read portion lock.
    ///
    /// # Safety
    ///
    /// [`AsyncRWLock`] is read-locked and leaked via [`AsyncReadLockGuard::leak`].
    unsafe fn read_unlock(&self);

    /// Releases the write portion lock.
    ///
    /// # Safety
    ///
    /// - [`AsyncRWLock`] is write-locked and leaked via [`AsyncWriteLockGuard::leak`].
    ///
    /// - Only current task has an ownership of this `write-lock`.
    unsafe fn write_unlock(&self);

    /// Returns a reference to the inner value.
    ///
    /// # Safety
    ///
    /// - [`AsyncRWLock`] must be read-locked for at least the duration of the returned associated
    ///   [`guard`](Self::ReadLockGuard).
    unsafe fn get_read_locked(&self) -> Self::ReadLockGuard<'_>;

    /// Returns a mutable reference to the inner value.
    ///
    /// # Safety
    ///
    /// - [`AsyncRWLock`] must be write-locked for at least the duration of the returned associated
    ///   [`guard`](Self::WriteLockGuard).
    ///
    /// - Only current task has an ownership of this `write-lock`.
    unsafe fn get_write_locked(&self) -> Self::WriteLockGuard<'_>;
}
