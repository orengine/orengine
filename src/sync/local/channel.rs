use std::collections::VecDeque;
use std::future::Future;
use std::intrinsics::{unlikely};
use std::task::{Context, Poll};
use crate::Executor;
use crate::local::Local;
use crate::runtime::{local_executor, Task};

struct Inner<T> {
    storage: VecDeque<T>,
    is_closed: bool,
    capacity: usize,
    senders: VecDeque<Task>,
    receivers: VecDeque<Task>
}

// region futures

pub struct WaitLocalSend<T> {
    inner: Local<Inner<T>>,
    value: Option<T>
}

impl<T> WaitLocalSend<T> {
    #[inline(always)]
    fn new(value: T, inner: Local<Inner<T>>) -> Self {
        Self {
            inner,
            value: Some(value)
        }
    }
}

macro_rules! insert_value {
    ($this:expr, $inner:expr) => {
        {
            unsafe {
                $inner.storage.push_back($this.value.take().unwrap_unchecked());
            }
            Poll::Ready(Ok(()))
        }
    };
}

impl<T> Future for WaitLocalSend<T> {
    type Output = Result<(), T>;

    #[inline(always)]
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let inner = this.inner.get_mut();

        if unlikely(inner.is_closed) {
            return Poll::Ready(Err(unsafe { this.value.take().unwrap_unchecked() }));
        }

        if unlikely(inner.receivers.len() > 0) {
            unsafe { local_executor().spawn_local_task(inner.receivers.pop_front().unwrap_unchecked()); }
            return insert_value!(this, inner);
        }

        let len = inner.storage.len();
        if unlikely(len >= inner.capacity) {
            let task = unsafe { (cx.waker().as_raw().data() as *mut Task).read() };
            inner.senders.push_back(task);
            return Poll::Pending;
        }

        insert_value!(this, inner)
    }
}

pub struct WaitLocalRecv<T> {
    inner: Local<Inner<T>>
}

impl<T> WaitLocalRecv<T> {
    #[inline(always)]
    fn new(inner: Local<Inner<T>>) -> Self {
        Self {
            inner
        }
    }
}

macro_rules! get_value {
    ($this:expr, $inner:expr) => {
        Poll::Ready(Ok(unsafe { ($inner.storage.pop_front().unwrap_unchecked()) }))
    };
}

impl<T> Future for WaitLocalRecv<T> {
    type Output = Result<T, ()>;

    #[inline(always)]
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let inner = this.inner.get_mut();

        if unlikely(inner.is_closed) {
            return Poll::Ready(Err(()));
        }

        if unlikely(inner.senders.len() > 0) {
            unsafe { local_executor().spawn_local_task(inner.senders.pop_front().unwrap_unchecked()); }
            return get_value!(this, inner);
        }

        let l = inner.storage.len();
        if unlikely(l == 0) {
            let task = unsafe { (cx.waker().as_raw().data() as *mut Task).read() };
            inner.receivers.push_back(task);
            return Poll::Pending;
        }

        get_value!(this, inner)
    }
}

// endregion

#[inline(always)]
fn close<T>(inner: &mut Inner<T>) {
    inner.is_closed = true;

    for task in inner.senders.drain(..) {
        Executor::exec_task(task);
    }

    for task in inner.receivers.drain(..) {
        Executor::exec_task(task);
    }
}

// region sender

pub struct LocalSender<T> {
    inner: Local<Inner<T>>
}

impl<T> LocalSender<T> {
    #[inline(always)]
    fn new(inner: Local<Inner<T>>) -> Self {
        Self {
            inner
        }
    }

    #[inline(always)]
    pub fn send(&self, value: T) -> WaitLocalSend<T> {
        WaitLocalSend::new(value, self.inner.clone())
    }

    #[inline(always)]
    pub fn close(self) {
        let inner = self.inner.get_mut();
        close(inner);
    }
}

impl<T> Clone for LocalSender<T> {
    fn clone(&self) -> Self {
        LocalSender {
            inner: self.inner.clone()
        }
    }
}

unsafe impl<T> Sync for LocalSender<T> {}
impl<T> !Send for LocalSender<T> {}

// endregion

// region receiver

pub struct LocalReceiver<T> {
    inner: Local<Inner<T>>
}

impl<T> LocalReceiver<T> {
    #[inline(always)]
    fn new(inner: Local<Inner<T>>) -> Self {
        Self {
            inner
        }
    }

    #[inline(always)]
    pub fn recv(&self) -> WaitLocalRecv<T> {
        WaitLocalRecv::new(self.inner.clone())
    }

    #[inline(always)]
    pub fn close(self) {
        let inner = self.inner.get_mut();
        close(inner);
    }
}

impl<T> Clone for LocalReceiver<T> {
    fn clone(&self) -> Self {
        LocalReceiver {
            inner: self.inner.clone()
        }
    }
}

unsafe impl<T> Sync for LocalReceiver<T> {}
impl<T> !Send for LocalReceiver<T> {}

// endregion

// region channel

pub struct LocalChannel<T> {
    inner: Local<Inner<T>>
}

impl<T> LocalChannel<T> {
    #[inline(always)]
    pub fn new(capacity: usize) -> Self {
        let real_capacity = if capacity == 0 { 1 } else { capacity };
        Self {
            inner: Local::new(Inner {
                storage: VecDeque::with_capacity(real_capacity),
                capacity,
                is_closed: false,
                senders: VecDeque::with_capacity(0),
                receivers: VecDeque::with_capacity(0)
            })
        }
    }

    #[inline(always)]
    pub fn send(&self, value: T) -> WaitLocalSend<T> {
        WaitLocalSend::new(value, self.inner.clone())
    }

    #[inline(always)]
    pub fn recv(&self) -> WaitLocalRecv<T> {
        WaitLocalRecv::new(self.inner.clone())
    }

    #[inline(always)]
    pub fn close(self) {
        let inner = self.inner.get_mut();
        close(inner);
    }

    #[inline(always)]
    pub fn split(self) -> (LocalSender<T>, LocalReceiver<T>) {
        (LocalSender::new(self.inner.clone()), LocalReceiver::new(self.inner))
    }
}

impl<T> Clone for LocalChannel<T> {
    fn clone(&self) -> Self {
        LocalChannel {
            inner: self.inner.clone()
        }
    }
}

unsafe impl<T> Sync for LocalChannel<T> {}
impl<T> !Send for LocalChannel<T> {}

// endregion

#[cfg(test)]
mod tests {
    use crate::yield_now;
    use super::*;

    #[test_macro::test]
    fn test_zero_capacity() {
        let ch = LocalChannel::new(0);
        let ch2 = ch.clone();
        let is_waiting = Local::new(false);
        let is_waiting2 = is_waiting.clone();

        local_executor().spawn_local(async move {
            ch.send(1).await.expect("closed");
            assert!(is_waiting.get());

            yield_now().await;

            ch.close();
        });

        *is_waiting2.get_mut() = true;
        let res = ch2.recv().await.expect("closed");
        assert_eq!(res, 1);
        assert!(is_waiting2.get());
        *is_waiting2.get_mut() = false;

        match ch2.send(2).await {
            Err(_) => assert!(true),
            _ => panic!("should be closed")
        };
    }

    const N: usize = 10_025;

    // case 1 - send N and recv N. No wait
    // case 2 - send N and recv (N + 1). Wait for recv
    // case 3 - send (N + 1) and recv N. Wait for send
    // case 4 - send (N + 1) and recv (N + 1). Wait for send and wait for recv

    #[test_macro::test]
    fn test_case1() {
        let ch = LocalChannel::new(N);
        let ch2 = ch.clone();

        local_executor().spawn_local(async move {
            for i in 0..N {
                ch.send(i).await.expect("closed");
            }

            yield_now().await;

            ch.close();
        });

        for i in 0..N {
            let res = ch2.recv().await.expect("closed");
            assert_eq!(res, i);
        }

        match ch2.recv().await {
            Err(_) => assert!(true),
            _ => panic!("should be closed")
        };
    }

    #[test_macro::test]
    fn test_case2() {
        let ch = LocalChannel::new(N);
        let ch2 = ch.clone();

        local_executor().spawn_local(async move {
            for i in 0..=N {
                let res = ch.recv().await.expect("closed");
                assert_eq!(res, i);
            }
        });

        for i in 0..N {
            let _ = ch2.send(i).await.expect("closed");
        }

        yield_now().await;

        let _ = ch2.send(N).await.expect("closed");
    }

    #[test_macro::test]
    fn test_case3() {
        let ch = LocalChannel::new(N);
        let ch2 = ch.clone();

        local_executor().spawn_local(async move {
            for i in 0..N {
                let res = ch.recv().await.expect("closed");
                assert_eq!(res, i);
            }

            yield_now().await;

            let res = ch.recv().await.expect("closed");
            assert_eq!(res, N);
        });

        for i in 0..=N {
            ch2.send(i).await.expect("closed");
        }
    }

    #[test_macro::test]
    fn test_case4() {
        let ch = LocalChannel::new(N);
        let ch2 = ch.clone();

        local_executor().spawn_local(async move {
            for i in 0..=N {
                let res = ch.recv().await.expect("closed");
                assert_eq!(res, i);
            }
        });

        for i in 0..=N {
            ch2.send(i).await.expect("closed");
        }
    }

    #[test_macro::test]
    fn test_split() {
        let (tx, rx) = LocalChannel::new(N).split();

        local_executor().spawn_local(async move {
            for i in 0..=N {
                let res = rx.recv().await.expect("closed");
                assert_eq!(res, i);
            }
        });

        for i in 0..=N {
            tx.send(i).await.expect("closed");
        }
    }
}