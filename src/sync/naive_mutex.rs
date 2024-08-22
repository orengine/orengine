use std::cell::UnsafeCell;
use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::task::{Context, Poll};
use crossbeam::utils::CachePadded;

pub struct MutexGuard<'mutex, T> {
    mutex: &'mutex Mutex<T>
}

impl<'mutex, T> MutexGuard<'mutex, T> {
    #[inline(always)]
    pub(crate) fn new(mutex: &'mutex Mutex<T>) -> Self {
        Self { mutex }
    }

    #[inline(always)]
    pub fn local_mutex(&self) -> &Mutex<T> {
        &self.mutex
    }

    #[inline(always)]
    pub(crate) fn into_local_mutex(self) -> &'mutex Mutex<T> {
        self.mutex
    }
}

impl<'mutex, T> Deref for MutexGuard<'mutex, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mutex.value.get() }
    }
}

impl<'mutex, T> DerefMut for MutexGuard<'mutex, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.value.get() }
    }
}

impl<'mutex, T> Drop for MutexGuard<'mutex, T> {
    fn drop(&mut self) {
        self.mutex.drop_lock();
    }
}

pub struct MutexWait<'mutex, T> {
    mutex: &'mutex Mutex<T>
}

impl<'mutex, T> MutexWait<'mutex, T> {
    #[inline(always)]
    fn new(local_mutex: &'mutex Mutex<T>) -> Self {
        Self {
            mutex: local_mutex
        }
    }
}

impl<'mutex, T> Future for MutexWait<'mutex, T> {
    type Output = MutexGuard<'mutex, T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        match this.mutex.try_lock() {
            Some(guard) => Poll::Ready(guard),
            None => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
}

pub struct Mutex<T> {
    is_locked: CachePadded<AtomicBool>,
    value: UnsafeCell<T>
}

impl<T> Mutex<T> {
    #[inline(always)]
    pub fn new(value: T) -> Mutex<T> {
        Mutex {
            is_locked: CachePadded::new(AtomicBool::new(false)),
            value: UnsafeCell::new(value)
        }
    }

    #[inline(always)]
    pub fn lock(&self) -> MutexWait<T> {
        MutexWait::new(self)
    }

    #[inline(always)]
    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        if self.is_locked.compare_exchange(false, true, Acquire, Relaxed).is_ok() {
            Some(MutexGuard::new(self))
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    #[inline(always)]
    fn drop_lock(&self) {
        self.is_locked.store(false, Release);
    }
}

unsafe impl<T: Send> Sync for Mutex<T> {}
unsafe impl<T: Send> Send for Mutex<T> {}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};
    use crate::end::end;
    use crate::runtime::create_local_executer_for_block_on;
    use crate::sleep::sleep;
    use crate::sync::WaitGroup;
    use crate::utils::global_test_lock::GLOBAL_TEST_LOCK;
    use super::*;

    #[test]
    fn test_mutex() {
        const SLEEP_DURATION: Duration = Duration::from_millis(1);

        let lock = GLOBAL_TEST_LOCK.lock("test_mutex".to_string());

        let mutex = Arc::new(Mutex::new(false));
        let wg = Arc::new(WaitGroup::new());
        let mutex_clone = mutex.clone();
        let wg_clone = wg.clone();
        create_local_executer_for_block_on(async move {
            wg.add(1);
            thread::spawn(move || {
               create_local_executer_for_block_on(async move {
                   let mut value = mutex_clone.lock().await;
                   wg_clone.done();
                   println!("1");
                   sleep(SLEEP_DURATION).await;
                   println!("3");
                   *value = true;
               });
            });

            wg.wait().await;
            let start = Instant::now();
            println!("2");
            let value = mutex.lock().await;
            println!("4");

            assert_eq!(*value, true);
            drop(value);

            end();
        });

        drop(lock);
    }

    #[test]
    fn test_try_mutex() {
        const SLEEP_DURATION: Duration = Duration::from_millis(1);

        let lock = GLOBAL_TEST_LOCK.lock("test_try_mutex".to_string());

        create_local_executer_for_block_on(async move {
            let mutex = Arc::new(Mutex::new(false));
            let wg = Arc::new(WaitGroup::new());
            let mutex_clone = mutex.clone();
            let wg_clone = wg.clone();

            wg.add(1);
            thread::spawn(move || {
               create_local_executer_for_block_on(async move {
                   let mut value = mutex_clone.lock().await;
                   wg_clone.done();
                   println!("1");
                   sleep(SLEEP_DURATION).await;
                   println!("4");
                   *value = true;
               });
            });

            wg.wait().await;
            println!("2");
            let value = mutex.try_lock();
            println!("3");
            assert!(value.is_none());

            sleep(SLEEP_DURATION * 2).await;

            let value = mutex.try_lock();
            println!("5");
            assert_eq!(*(value.expect("not waited")), true);

            end();
        });

        drop(lock);
    }

    #[test]
    fn naive_test_mutex() {
        const SLEEP_DURATION: Duration = Duration::from_micros(1);
        const PAR: usize = 100;
        const TRIES: usize = 200;

        let lock = GLOBAL_TEST_LOCK.lock("naive test mutex".to_string());
        let mutex = Arc::new(Mutex::new(0));
        for _ in 1..PAR {
            let mutex = mutex.clone();
            thread::spawn(move || {
                create_local_executer_for_block_on(async move {
                    for _ in 0..TRIES {
                        let mut lock = mutex.lock().await;
                        //let mut lock = mutex.lock().unwrap();
                        sleep(SLEEP_DURATION).await;
                        *lock += 1;
                        if *lock % 100 == 0 {
                            println!("{} of {}", *lock, TRIES * PAR);
                        }
                    }
                });
            });
        }

        create_local_executer_for_block_on(async move {
            for _ in 0..TRIES {
                //let mut lock = mutex.lock().await;
                let mut lock = mutex.lock().await;
                sleep(SLEEP_DURATION).await;
                *lock += 1;
            }

            loop {
                sleep(SLEEP_DURATION).await;
                let lock = mutex.lock().await;
                if *lock == PAR * TRIES {
                    end();
                }
            }
        });

        drop(lock);
    }
}