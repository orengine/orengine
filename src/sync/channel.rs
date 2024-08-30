use std::collections::VecDeque;
use std::future::Future;
use std::intrinsics::{unlikely};
use std::mem::MaybeUninit;
use std::{mem, ptr};
use std::task::{Context, Poll};
use crate::runtime::{local_executor, Task};
use crate::utils::SpinLock;

struct Inner<T> {
    storage: VecDeque<T>,
    is_closed: bool,
    capacity: usize,
    senders: VecDeque<Task>,
    receivers: VecDeque<(Task, *mut T)>
}

// region futures

macro_rules! return_pending_and_release_lock {
    ($ex:expr, $lock:expr) => {
        unsafe { $ex.release_atomic_bool($lock.leak()) };
        return Poll::Pending;
    };
}

pub struct WaitSend<'future, T> {
    inner: &'future SpinLock<Inner<T>>,
    value: Option<T>
}

impl<'future, T> WaitSend<'future, T> {
    #[inline(always)]
    fn new(value: T, inner: &'future SpinLock<Inner<T>>) -> Self {
        Self {
            inner,
            value: Some(value)
        }
    }
}

macro_rules! insert_value {
    ($this:expr, $inner_lock:expr) => {
        {
            unsafe {
                $inner_lock.storage.push_back($this.value.take().unwrap_unchecked());
            }
            Poll::Ready(Ok(()))
        }
    };
}

impl<'future, T> Future for WaitSend<'future, T> {
    type Output = Result<(), T>;

    #[inline(always)]
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let mut inner_lock = this.inner.lock();
        let ex = local_executor();

        if unlikely(inner_lock.is_closed) {
            return Poll::Ready(Err(unsafe { this.value.take().unwrap_unchecked() }));
        }

        if unlikely(inner_lock.receivers.len() > 0) {
            let (task, slot) = unsafe {
                inner_lock.receivers.pop_front().unwrap_unchecked()
            };
            unsafe { *slot = this.value.take().unwrap_unchecked() };
            local_executor().exec_task(task); // here we move the lock (by wake the receiver task up)
            mem::forget(inner_lock); // we have moved to a receiver the lock above
            return Poll::Ready(Ok(()));
        }

        let len = inner_lock.storage.len();
        if unlikely(len >= inner_lock.capacity) {
            let task = unsafe { (cx.waker().as_raw().data() as *mut Task).read() };
            inner_lock.senders.push_back(task);
            return_pending_and_release_lock!(ex, inner_lock);
        }

        insert_value!(this, inner_lock)
    }
}

pub struct WaitRecv<'future, T> {
    inner: &'future SpinLock<Inner<T>>,
    was_enqueued: bool,
    slot: &'future mut T
}

impl<'future, T> WaitRecv<'future, T> {
    #[inline(always)]
    fn new(inner: &'future SpinLock<Inner<T>>, slot: &'future mut T) -> Self {
        Self {
            inner,
            was_enqueued: false,
            slot
        }
    }
}

macro_rules! get_value {
    ($this:expr, $inner_lock:expr) => {
        Poll::Ready(Ok(unsafe {
            ptr::write($this.slot, $inner_lock.storage.pop_front().unwrap_unchecked())
        }))
    };
}

impl<'future, T> Future for WaitRecv<'future, T> {
    type Output = Result<(), ()>;

    #[inline(always)]
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let ex = local_executor();

        if unlikely(this.was_enqueued) {
            // here we have an ownership of the lock
            let inner_lock = unsafe { this.inner.get_locked() };
            if unlikely(inner_lock.is_closed) {
                // here we don't release the lock, because this task have been woken up by a closing
                return Poll::Ready(Err(()));
            }

            // here we release the lock, because this task have been woken up by a sender
            unsafe { this.inner.unlock(); }

            return Poll::Ready(Ok(()));
        }

        let mut inner_lock = this.inner.lock();

        if unlikely(inner_lock.is_closed) {
            return Poll::Ready(Err(()));
        }

        if unlikely(inner_lock.senders.len() > 0) {
            unsafe {
                ex.spawn_local_task(inner_lock.senders.pop_front().unwrap_unchecked());
            }
        }

        let l = inner_lock.storage.len();
        if unlikely(l == 0) {
            let task = unsafe { (cx.waker().as_raw().data() as *mut Task).read() };
            this.was_enqueued = true;
            inner_lock.receivers.push_back((task, this.slot));
            return_pending_and_release_lock!(ex, inner_lock);
        }

        get_value!(this, inner_lock)
    }
}

// endregion

#[inline(always)]
fn close<T>(inner: &SpinLock<Inner<T>>) {
    let mut inner_lock = inner.lock();
    inner_lock.is_closed = true;
    let executor = local_executor();

    for task in inner_lock.senders.drain(..) {
        executor.exec_task(task);
    }

    for (task, _) in inner_lock.receivers.drain(..) {
        executor.exec_task(task);
    }
}

// region sender

pub struct Sender<'channel, T> {
    inner: &'channel SpinLock<Inner<T>>
}

impl<'channel, T> Sender<'channel, T> {
    #[inline(always)]
    fn new(inner: &'channel SpinLock<Inner<T>>) -> Self {
        Self {
            inner
        }
    }

    #[inline(always)]
    pub fn send(&self, value: T) -> WaitSend<'_, T> {
        WaitSend::new(value, self.inner)
    }

    #[inline(always)]
    pub fn close(&self) {
        close(self.inner);
    }
}

impl<'channel, T> Clone for Sender<'channel, T> {
    fn clone(&self) -> Self {
        Sender {
            inner: self.inner
        }
    }
}

unsafe impl<'channel, T> Sync for Sender<'channel, T> {}
unsafe impl<'channel, T> Send for Sender<'channel, T> {}

// endregion

// region receiver

pub struct Receiver<'channel, T> {
    inner: &'channel SpinLock<Inner<T>>
}

impl<'channel, T> Receiver<'channel, T> {
    #[inline(always)]
    fn new(inner: &'channel SpinLock<Inner<T>>) -> Self {
        Self {
            inner
        }
    }

    #[inline(always)]
    pub async fn recv(&self) -> Result<T, ()> {
        let mut slot = MaybeUninit::uninit();
        unsafe {
            match self.recv_in(&mut *slot.as_mut_ptr()).await {
                Ok(_) => Ok(slot.assume_init()),
                Err(_) => Err(()),
            }
        }
    }

    #[inline(always)]
    pub fn recv_in<'future>(&'future self, slot: &'future mut T) -> WaitRecv<'future, T> {
        WaitRecv::new(self.inner, slot)
    }

    #[inline(always)]
    pub fn close(self) {
        close(self.inner);
    }
}

impl<'channel, T> Clone for Receiver<'channel, T> {
    fn clone(&self) -> Self {
        Receiver {
            inner: self.inner
        }
    }
}

unsafe impl<'channel, T> Sync for Receiver<'channel, T> {}
unsafe impl<'channel, T> Send for Receiver<'channel, T> {}

// endregion

// region channel

pub struct Channel<T> {
    inner: SpinLock<Inner<T>>
}

impl<T> Channel<T> {
    #[inline(always)]
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: SpinLock::new(Inner {
                storage: VecDeque::with_capacity(capacity),
                capacity,
                is_closed: false,
                senders: VecDeque::with_capacity(0),
                receivers: VecDeque::with_capacity(0)
            })
        }
    }

    #[inline(always)]
    pub fn send(&self, value: T) -> WaitSend<T> {
        WaitSend::new(value, &self.inner)
    }

    #[inline(always)]
    pub async fn recv(&self) -> Result<T, ()> {
        let mut slot = MaybeUninit::uninit();
        unsafe {
            match self.recv_in(&mut *slot.as_mut_ptr()).await {
                Ok(_) => Ok(slot.assume_init()),
                Err(_) => Err(()),
            }
        }
    }

    #[inline(always)]
    pub fn recv_in<'future>(&'future self, slot: &'future mut T) -> WaitRecv<'future, T> {
        WaitRecv::new(&self.inner, slot)
    }

    #[inline(always)]
    pub fn close(&self) {
        close(&self.inner);
    }

    #[inline(always)]
    pub fn split(&self) -> (Sender<T>, Receiver<T>) {
        (Sender::new(&self.inner), Receiver::new(&self.inner))
    }
}

unsafe impl<T> Sync for Channel<T> {}
unsafe impl<T> Send for Channel<T> {}

// endregion