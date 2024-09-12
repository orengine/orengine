use std::sync::{Mutex, MutexGuard};

pub(crate) struct TestLock {
    lock: Mutex<()>
}

impl TestLock {
    pub const fn new() -> Self {
        Self {
            lock: Mutex::new(())
        }
    }

    pub fn lock(&'static self) -> MutexGuard<()> {
        self.lock.lock().unwrap()
    }
}

pub(crate) static GLOBAL_TEST_LOCK: TestLock = TestLock::new();