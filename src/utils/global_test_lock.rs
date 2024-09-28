use std::sync::{Mutex, MutexGuard};

/// A global lock that can be used to prevent tests from running in parallel.
pub(crate) struct TestLock {
    lock: Mutex<()>
}

impl TestLock {
    /// Create a new [`TestLock`].
    pub(crate) const fn new() -> Self {
        Self {
            lock: Mutex::new(())
        }
    }

    /// Lock the [`TestLock`].
    pub(crate) fn lock(&'static self) -> MutexGuard<'static, ()> {
        self.lock.lock().unwrap()
    }
}

/// A global lock that can be used to prevent tests from running in parallel.
pub(crate) static GLOBAL_TEST_LOCK: TestLock = TestLock::new();