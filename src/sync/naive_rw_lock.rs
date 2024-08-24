use std::cell::UnsafeCell;
use std::intrinsics::unlikely;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};

use crossbeam::utils::CachePadded;

use crate::yield_now;

// region guards

pub struct ReadLockGuard<'rw_lock, T> {
    local_rw_lock: &'rw_lock RWLock<T>,
}

impl<'rw_lock, T> ReadLockGuard<'rw_lock, T> {
    #[inline(always)]
    fn new(local_rw_lock: &'rw_lock RWLock<T>) -> Self {
        Self { local_rw_lock }
    }
}

impl<'rw_lock, T> Deref for ReadLockGuard<'rw_lock, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.local_rw_lock.value.get() }
    }
}

impl<'rw_lock, T> Drop for ReadLockGuard<'rw_lock, T> {
    fn drop(&mut self) {
        self.local_rw_lock.drop_read();
    }
}

pub struct WriteLockGuard<'rw_lock, T> {
    local_rw_lock: &'rw_lock RWLock<T>,
}

impl<'rw_lock, T> WriteLockGuard<'rw_lock, T> {
    #[inline(always)]
    fn new(local_rw_lock: &'rw_lock RWLock<T>) -> Self {
        Self { local_rw_lock }
    }
}

impl<'rw_lock, T> Deref for WriteLockGuard<'rw_lock, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.local_rw_lock.value.get() }
    }
}

impl<'rw_lock, T> DerefMut for WriteLockGuard<'rw_lock, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.local_rw_lock.value.get() }
    }
}

impl<'rw_lock, T> Drop for WriteLockGuard<'rw_lock, T> {
    fn drop(&mut self) {
        self.local_rw_lock.drop_write();
    }
}

// endregion

pub struct RWLock<T> {
    number_of_readers: CachePadded<AtomicIsize>,
    value: UnsafeCell<T>,
}

impl<T> RWLock<T> {
    #[inline(always)]
    pub fn new(value: T) -> RWLock<T> {
        RWLock {
            number_of_readers: CachePadded::new(AtomicIsize::new(0)),
            value: UnsafeCell::new(value),
        }
    }

    #[inline(always)]
    pub async fn write(&self) -> WriteLockGuard<T> {
        loop {
            match self.try_write() {
                Some(guard) => return guard,
                None => yield_now().await,
            }
        }
    }

    #[inline(always)]
    pub fn try_write(&self) -> Option<WriteLockGuard<T>> {
        if unlikely(
            self.number_of_readers
                .compare_exchange(0, -1, Acquire, Relaxed)
                .is_ok(),
        ) {
            Some(WriteLockGuard::new(self))
        } else {
            None
        }
    }

    #[inline(always)]
    pub async fn read(&self) -> ReadLockGuard<T> {
        loop {
            match self.try_read() {
                Some(guard) => return guard,
                None => yield_now().await,
            }
        }
    }

    #[inline(always)]
    pub fn try_read(&self) -> Option<ReadLockGuard<T>> {
        loop {
            let number_of_readers = self.number_of_readers.load(Acquire);
            if unlikely(number_of_readers < 0) {
                break None;
            } else {
                if self
                    .number_of_readers
                    .compare_exchange(number_of_readers, number_of_readers + 1, Acquire, Relaxed)
                    .is_ok()
                {
                    break Some(ReadLockGuard::new(self));
                }
            }
        }
    }

    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    #[inline(always)]
    fn drop_read(&self) {
        self.number_of_readers.fetch_sub(1, Release);
    }

    #[inline(always)]
    fn drop_write(&self) {
        self.number_of_readers.store(0, Release);
    }
}

unsafe impl<T: Send> Sync for RWLock<T> {}
unsafe impl<T: Send> Send for RWLock<T> {}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering::SeqCst;
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    use crate::runtime::create_local_executer_for_block_on;
    use crate::sleep::sleep;
    use crate::sync::WaitGroup;
    use crate::Executor;

    use super::*;

    #[test_macro::test]
    fn test_rw_lock() {
        const SLEEP_DURATION: Duration = Duration::from_millis(1);

        let start = Instant::now();
        let mutex = Arc::new(RWLock::new(0));
        let wg = Arc::new(WaitGroup::new());
        let read_wg = Arc::new(WaitGroup::new());

        for _ in 1..=10 {
            let mutex = mutex.clone();
            let wg = wg.clone();
            wg.add(1);

            thread::spawn(move || {
                create_local_executer_for_block_on(async move {
                    let value = mutex.read().await;
                    assert_eq!(*value, 0);
                    wg.done();
                    sleep(SLEEP_DURATION).await;
                    assert_eq!(*value, 0);
                });
            });
        }

        wg.wait().await;

        for _ in 1..=10 {
            let wg = wg.clone();
            let read_wg = read_wg.clone();
            wg.add(1);
            let mutex = mutex.clone();

            thread::spawn(move || {
                create_local_executer_for_block_on(async move {
                    assert_eq!(mutex.number_of_readers.load(SeqCst), 10);
                    let mut value = mutex.write().await;
                    {
                        let read_wg = read_wg.clone();
                        let mutex = mutex.clone();
                        read_wg.add(1);

                        Executor::exec_future(async move {
                            assert_eq!(mutex.number_of_readers.load(SeqCst), -1);
                            let value = mutex.read().await;
                            assert_ne!(*value, 0);
                            assert_ne!(mutex.number_of_readers.load(SeqCst), 0);
                            read_wg.done();
                        });
                    }
                    let elapsed = start.elapsed();
                    assert!(elapsed >= SLEEP_DURATION);
                    assert_eq!(mutex.number_of_readers.load(SeqCst), -1);
                    *value += 1;

                    wg.done();
                });
            });
        }

        wg.wait().await;
        read_wg.wait().await;

        let value = mutex.read().await;
        assert_eq!(*value, 10);
        assert_ne!(mutex.number_of_readers.load(SeqCst), 0);
    }
}
