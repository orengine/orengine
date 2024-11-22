/// Represents the state of a lock in a [`read-write lock`](crate::sync::AsyncRWLock) mechanism.
///
/// # Variants
///
/// * `Unlocked`:  
///   Indicates that the lock is currently not held by any
///   [`readers`](crate::sync::AsyncReadLockGuard) or writers.
///   
/// * `WriteLocked`:  
///   Represents that the lock is currently held in write mode.  
///   Only one task can acquire the lock in this state, ensuring exclusive access.
///
/// * `ReadLocked(usize)`:  
///   Indicates that the lock is currently held in read mode by one or more
///   [`readers`](crate::sync::AsyncReadLockGuard).
///   
///   The `usize` value represents the number of active [`readers`](crate::sync::AsyncReadLockGuard)
///   holding the lock.
#[derive(Debug, Eq, PartialEq)]
pub enum LockStatus {
    /// Indicates that the lock is currently not held by any
    /// [`readers`](crate::sync::AsyncReadLockGuard) or writers.
    Unlocked,
    /// Represents that the lock is currently held in write mode.
    /// Only one task can acquire the lock in this state, ensuring exclusive access.
    WriteLocked,
    /// Indicates that the lock is currently held in read mode by one or more
    /// [`readers`](crate::sync::AsyncReadLockGuard).
    ///
    /// The `usize` value represents the number of active [`readers`](crate::sync::AsyncReadLockGuard)
    /// holding the lock.
    ReadLocked(usize),
}
