use std::cell::UnsafeCell;
use std::fmt::Debug;
use std::intrinsics::likely;
use std::mem::ManuallyDrop;
use std::ptr;
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};

use crossbeam::utils::CachePadded;

use crate::yield_now;

union Data<T, F> {
    value: ManuallyDrop<T>,
    f: ManuallyDrop<F>,
}

pub enum LazyLockState {
    NotCalled = 0,
    InProgress = 1,
    Ready = 2,
}

impl LazyLockState {
    pub const fn not_called() -> isize {
        0
    }

    pub const fn in_progress() -> isize {
        1
    }

    pub const fn ready() -> isize {
        2
    }
}

impl From<isize> for LazyLockState {
    fn from(v: isize) -> Self {
        match v {
            0 => Self::NotCalled,
            1 => Self::InProgress,
            2 => Self::Ready,
            _ => {
                panic!(
                    "Invalid LazyLockState value: {}.\
                    It can only be 0 (NotCalled), 1 (InProgress), 1 or 2 (Ready).",
                    v
                )
            }
        }
    }
}

impl Into<isize> for LazyLockState {
    fn into(self) -> isize {
        self as isize
    }
}

pub struct LazyLock<T, F: FnOnce() -> T = fn() -> T> {
    state: CachePadded<AtomicIsize>,
    data: UnsafeCell<Data<T, F>>,
}

impl<T, F: FnOnce() -> T> LazyLock<T, F> {
    pub const fn new(f: F) -> Self {
        Self {
            state: CachePadded::new(AtomicIsize::new(LazyLockState::not_called())),
            data: UnsafeCell::new(Data {
                f: ManuallyDrop::new(f),
            }),
        }
    }

    #[inline(always)]
    pub async fn into_inner(this: Self) -> Result<T, F> {
        let this = ManuallyDrop::new(this);
        let data = unsafe { ptr::read(&this.data) }.into_inner();
        match this.state.load(Relaxed).into() {
            LazyLockState::NotCalled => Err(ManuallyDrop::into_inner(unsafe { data.f })),
            LazyLockState::Ready => Ok(ManuallyDrop::into_inner(unsafe { data.value })),
            _ => unreachable!(), // here this is owned only by the current thread, so it can't be in progress
        }
    }

    #[inline(always)]
    pub async fn deref(&self) -> &T {
        loop {
            match self.state.load(Relaxed).into() {
                LazyLockState::Ready => unsafe {
                    &*(*self.data.get()).value;
                },
                LazyLockState::InProgress => yield_now().await,
                LazyLockState::NotCalled => {
                    if likely(
                        self.state
                            .compare_exchange(
                                LazyLockState::not_called(),
                                LazyLockState::in_progress(),
                                Acquire,
                                Relaxed,
                            )
                            .is_ok(),
                    ) {
                        let data = unsafe { &mut *self.data.get() };
                        let f = unsafe { ManuallyDrop::take(&mut data.f) };
                        let value = f();
                        data.value = ManuallyDrop::new(value);

                        self.state.store(LazyLockState::ready(), Release);
                        return unsafe { &*(*self.data.get()).value };
                    }

                    // try again
                }
            }
        }
    }

    #[inline(always)]
    pub fn get(&self) -> Option<&T> {
        match self.state.load(Relaxed).into() {
            LazyLockState::Ready => Some(unsafe { &*(*self.data.get()).value }),
            _ => None,
        }
    }
}

impl<T: Default> Default for LazyLock<T> {
    fn default() -> Self {
        Self::new(T::default)
    }
}

impl<T: Debug, F: FnOnce() -> T> Debug for LazyLock<T, F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_tuple("LazyLock");
        match self.get() {
            Some(v) => d.field(v),
            None => d.field(&format_args!("<uninit>")),
        };
        d.finish()
    }
}

unsafe impl<T: Sync + Send, F: Send + FnOnce() -> T> Sync for LazyLock<T, F> {}

impl<T, F: FnOnce() -> T> Drop for LazyLock<T, F> {
    fn drop(&mut self) {
        unsafe {
            match self.state.load(Relaxed).into() {
                LazyLockState::Ready => ManuallyDrop::drop(&mut self.data.get_mut().value),
                _ => ManuallyDrop::drop(&mut self.data.get_mut().f),
            }
        }
    }
}
