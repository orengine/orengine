use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

pub(crate) struct TestLock {
    lock: Mutex<usize>
}

impl TestLock {
    pub const fn new() -> Self {
        Self {
            lock: Mutex::new(0)
        }
    }

    pub fn lock(&'static self, name: String) -> MutexGuard<usize> {
        let mut guard = self.lock.lock().unwrap();

        *guard += 1;
        thread::spawn(move || {
            println!("test {} start", name);
            for _ in 0..10_000 {
                thread::sleep(Duration::from_millis(1));
                let guard = self.lock.try_lock();
                if guard.is_ok() {
                    println!("test {} is finished!", name);
                    return;
                }
            }

            panic!("{} is not finished in 10 seconds!", name);
        });

        guard
    }
}


pub(crate) static GLOBAL_TEST_LOCK: TestLock = TestLock::new();