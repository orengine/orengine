use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::future::Future;
use std::intrinsics::{unlikely};
use std::mem::MaybeUninit;
use std::ptr;
use std::task::{Context, Poll};
use crate::runtime::{local_executor, Task};

struct Inner<T> {
    storage: VecDeque<T>,
    is_closed: bool,
    capacity: usize,
    senders: VecDeque<Task>,
    receivers: VecDeque<(Task, *mut T)>
}

// region futures

pub struct WaitLocalSend<'future, T> {
    inner: &'future mut Inner<T>,
    value: Option<T>
}

impl<'future, T> WaitLocalSend<'future, T> {
    #[inline(always)]
    fn new(value: T, inner: &'future mut Inner<T>) -> Self {
        Self {
            inner,
            value: Some(value)
        }
    }
}

macro_rules! insert_value {
    ($this:expr) => {
        {
            unsafe {
                $this.inner.storage.push_back($this.value.take().unwrap_unchecked());
            }
            Poll::Ready(Ok(()))
        }
    };
}

impl<'future, T> Future for WaitLocalSend<'future, T> {
    type Output = Result<(), T>;

    #[inline(always)]
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        if unlikely(this.inner.is_closed) {
            return Poll::Ready(Err(unsafe { this.value.take().unwrap_unchecked() }));
        }

        if unlikely(this.inner.receivers.len() > 0) {
            let (task, slot) = unsafe { this.inner.receivers.pop_front().unwrap_unchecked() };
            unsafe { *slot = this.value.take().unwrap_unchecked() };
            local_executor().exec_task(task);
            return Poll::Ready(Ok(()));
        }

        let len = this.inner.storage.len();
        if unlikely(len >= this.inner.capacity) {
            let task = unsafe { (cx.waker().as_raw().data() as *mut Task).read() };
            this.inner.senders.push_back(task);
            return Poll::Pending;
        }

        insert_value!(this)
    }
}

pub struct WaitLocalRecv<'future, T> {
    inner: &'future mut Inner<T>,
    was_enqueued: bool,
    slot: &'future mut T
}

impl<'future, T> WaitLocalRecv<'future, T> {
    #[inline(always)]
    fn new(inner: &'future mut Inner<T>, slot: &'future mut T) -> Self {
        Self {
            inner,
            was_enqueued: false,
            slot
        }
    }
}

macro_rules! get_value {
    ($this:expr) => {
        Poll::Ready(Ok(unsafe {
            ptr::write($this.slot, $this.inner.storage.pop_front().unwrap_unchecked())
        }))
    };
}

impl<'future, T> Future for WaitLocalRecv<'future, T> {
    type Output = Result<(), ()>;

    #[inline(always)]
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        if unlikely(this.inner.is_closed) {
            return Poll::Ready(Err(()));
        }

        if unlikely(this.was_enqueued) {
            return Poll::Ready(Ok(()));
        }

        if unlikely(this.inner.senders.len() > 0) {
            unsafe { local_executor().spawn_local_task(this.inner.senders.pop_front().unwrap_unchecked()); }
            return get_value!(this);
        }

        let l = this.inner.storage.len();
        if unlikely(l == 0) {
            let task = unsafe { (cx.waker().as_raw().data() as *mut Task).read() };
            this.was_enqueued = true;
            this.inner.receivers.push_back((task, this.slot));
            return Poll::Pending;
        }

        get_value!(this)
    }
}

// endregion

#[inline(always)]
fn close<T>(inner: &mut Inner<T>) {
    inner.is_closed = true;
    let executor = local_executor();

    for task in inner.senders.drain(..) {
        executor.exec_task(task);
    }

    for (task, _) in inner.receivers.drain(..) {
        executor.exec_task(task);
    }
}

// region sender

pub struct LocalSender<'channel, T> {
    inner: &'channel UnsafeCell<Inner<T>>
}

impl<'channel, T> LocalSender<'channel, T> {
    #[inline(always)]
    fn new(inner: &'channel UnsafeCell<Inner<T>>) -> Self {
        Self {
            inner
        }
    }

    #[inline(always)]
    pub fn send(&self, value: T) -> WaitLocalSend<'_, T> {
        WaitLocalSend::new(value, unsafe { &mut * self.inner.get() })
    }

    #[inline(always)]
    pub fn close(self) {
        let inner = unsafe { &mut * self.inner.get() };
        close(inner);
    }
}

impl<'channel, T> Clone for LocalSender<'channel, T> {
    fn clone(&self) -> Self {
        LocalSender {
            inner: self.inner
        }
    }
}

unsafe impl<'channel, T> Sync for LocalSender<'channel, T> {}
impl<'channel, T> !Send for LocalSender<'channel, T> {}

// endregion

// region receiver

pub struct LocalReceiver<'channel, T> {
    inner: &'channel UnsafeCell<Inner<T>>
}

impl<'channel, T> LocalReceiver<'channel, T> {
    #[inline(always)]
    fn new(inner: &'channel UnsafeCell<Inner<T>>) -> Self {
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
    pub fn recv_in<'future>(&self, slot: &'future mut T) -> WaitLocalRecv<'future, T> {
        WaitLocalRecv::new(unsafe { &mut * self.inner.get() }, slot)
    }

    #[inline(always)]
    pub fn close(self) {
        let inner = unsafe { &mut * self.inner.get() };
        close(inner);
    }
}

impl<'channel, T> Clone for LocalReceiver<'channel, T> {
    fn clone(&self) -> Self {
        LocalReceiver {
            inner: self.inner
        }
    }
}

unsafe impl<'channel, T> Sync for LocalReceiver<'channel, T> {}
impl<'channel, T> !Send for LocalReceiver<'channel, T> {}

// endregion

// region channel

pub struct LocalChannel<T> {
    inner: UnsafeCell<Inner<T>>
}

impl<T> LocalChannel<T> {
    #[inline(always)]
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: UnsafeCell::new(Inner {
                storage: VecDeque::with_capacity(capacity),
                capacity,
                is_closed: false,
                senders: VecDeque::with_capacity(0),
                receivers: VecDeque::with_capacity(0)
            })
        }
    }

    #[inline(always)]
    pub fn send(&self, value: T) -> WaitLocalSend<'_, T> {
        WaitLocalSend::new(value, unsafe { &mut * self.inner.get() })
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
    pub fn recv_in<'future>(&self, slot: &'future mut T) -> WaitLocalRecv<'future, T> {
        WaitLocalRecv::new(unsafe { &mut * self.inner.get() }, slot)
    }

    #[inline(always)]
    pub fn close(self) {
        let inner = unsafe { &mut * self.inner.get() };
        close(inner);
    }

    #[inline(always)]
    pub fn split(&self) -> (LocalSender<T>, LocalReceiver<T>) {
        (LocalSender::new(&self.inner), LocalReceiver::new(&self.inner))
    }
}

unsafe impl<T> Sync for LocalChannel<T> {}
impl<T> !Send for LocalChannel<T> {}

// endregion

// TODO
// #[cfg(test)]
// mod tests {
//     use crate::sync::WaitGroup;
//     use crate::yield_now;
//     use super::*;
//
//     #[test_macro::test]
//     fn test_zero_capacity() {
//         let ch = LocalChannel::new(0);
//
//         local_executor().spawn_local(async move {
//             ch.send(1).await.expect("closed");
//
//             yield_now().await;
//
//             ch.close();
//         });
//
//         let res = ch.recv().await.expect("closed");
//         assert_eq!(res, 1);
//
//         match ch.send(2).await {
//             Err(_) => assert!(true),
//             _ => panic!("should be closed")
//         };
//     }
//
//     const N: usize = 10_025;
//
//     // case 1 - send N and recv N. No wait
//     // case 2 - send N and recv (N + 1). Wait for recv
//     // case 3 - send (N + 1) and recv N. Wait for send
//     // case 4 - send (N + 1) and recv (N + 1). Wait for send and wait for recv
//
//     #[test_macro::test]
//     fn test_local_channel_case1() {
//         let ch = LocalChannel::new(N);
//
//         local_executor().spawn_local(async move {
//             for i in 0..N {
//                 ch.send(i).await.expect("closed");
//             }
//
//             yield_now().await;
//
//             ch.close();
//         });
//
//         for i in 0..N {
//             let res = ch.recv().await.expect("closed");
//             assert_eq!(res, i);
//         }
//
//         match ch.recv().await {
//             Err(_) => assert!(true),
//             _ => panic!("should be closed")
//         };
//     }
//
//     #[test_macro::test]
//     fn test_local_channel_case2() {
//         let ch = LocalChannel::new(N);
//
//         local_executor().spawn_local(async move {
//             for i in 0..=N {
//                 let res = ch.recv().await.expect("closed");
//                 assert_eq!(res, i);
//             }
//         });
//
//         for i in 0..N {
//             let _ = ch.send(i).await.expect("closed");
//         }
//
//         yield_now().await;
//
//         let _ = ch.send(N).await.expect("closed");
//     }
//
//     #[test_macro::test]
//     fn test_local_channel_case3() {
//         let ch = LocalChannel::new(N);
//
//         local_executor().spawn_local(async move {
//             for i in 0..N {
//                 let res = ch.recv().await.expect("closed");
//                 assert_eq!(res, i);
//             }
//
//             yield_now().await;
//
//             let res = ch.recv().await.expect("closed");
//             assert_eq!(res, N);
//         });
//
//         for i in 0..=N {
//             ch.send(i).await.expect("closed");
//         }
//     }
//
//     #[test_macro::test]
//     fn test_local_channel_case4() {
//         let ch = LocalChannel::new(N);
//
//         local_executor().spawn_local(async move {
//             for i in 0..=N {
//                 let res = ch.recv().await.expect("closed");
//                 assert_eq!(res, i);
//             }
//         });
//
//         for i in 0..=N {
//             ch.send(i).await.expect("closed");
//         }
//     }
//
//     #[test_macro::test]
//     fn test_local_channel_split() {
//         let (tx, rx) = LocalChannel::new(N).split();
//
//         local_executor().spawn_local(async move {
//             for i in 0..=N*2 {
//                 let res = rx.recv().await.expect("closed");
//                 assert_eq!(res, i);
//             }
//         });
//
//         let wg = WaitGroup::new();
//         wg.add(1);
//
//         local_executor().spawn_local(async move {
//             for i in 0..N {
//                 tx.send(i).await.expect("closed");
//             }
//
//             wg.done();
//         });
//
//         wg.wait().await;
//
//         for i in N..=N*2 {
//             tx.send(i).await.expect("closed");
//         }
//     }
// }