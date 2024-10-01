use std::collections::VecDeque;
use std::future::Future;
use std::mem::{ManuallyDrop, MaybeUninit};
use std::ops::Deref;
use std::ptr;
use std::ptr::drop_in_place;
use std::task::{Context, Poll};

use crate::panic_if_local_in_future;
use crate::runtime::{local_executor, Task};
use crate::sync::naive_mutex::NaiveMutex;

enum SendCallState<T> {
    /// Default state.
    ///
    /// # Lock note
    ///
    /// It has no lock now.
    FirstCall,
    /// This task was enqueued, now it is woken to write into queue,
    /// because a [`WaitRecv`] has read from the queue already.
    ///
    /// # Scenario
    ///
    /// 1 - sender acquire the lock
    ///
    /// 2 - sender can't write into the queue
    ///
    /// 3 - sender stand inside the senders queue
    ///
    /// 4 - sender drop the lock
    ///
    /// 5 - receiver acquire the lock
    ///
    /// 6 - receiver read from the queue
    ///
    /// 7 - receiver take a sender's task and put [`SendCallState::WokenToWriteIntoQueueWithLock`]
    ///
    /// 8 - receiver exec the sender's task
    ///
    /// 9 - sender write into the queue
    ///
    /// 10 - sender release the lock
    ///
    /// 11 - sender do its job
    ///
    /// 12 - after sender yield or return, receiver return [`Poll::Ready`].
    ///
    /// # Lock note
    ///
    /// It has a lock now.
    WokenToWriteIntoQueueWithLock,
    /// This task was enqueued, now it is woken to write into the slot,
    /// because this is a zero-capacity channel and a [`WaitRecv`] is waiting for read.
    ///
    /// # Scenario
    ///
    /// 1 - sender acquire the lock
    ///
    /// 2 - sender can't write into the queue
    ///
    /// 3 - sender stand inside the senders queue
    ///
    /// 4 - sender drop the lock
    ///
    /// 5 - receiver acquire the lock
    ///
    /// 6 - receiver can't read from the queue
    ///
    /// 7 - receiver take a sender's task and put [`SendCallState::WokenToWriteIntoTheSlot`]
    ///
    /// 8 - receiver drop the lock
    ///
    /// 9 - receiver exec the sender's task
    ///
    /// 10 - sender write into the slot and do its job
    ///
    /// 11 - after sender yield or return, receiver return [`Poll::Ready`].
    ///
    /// # Lock note
    ///
    /// It has no lock now.
    WokenToWriteIntoTheSlot(*mut T),
    /// This task was enqueued, now it is woken by close, and it has no lock now.
    WokenByClose,
}

#[repr(u8)]
enum RecvCallState {
    /// Default state.
    ///
    /// # Lock note
    ///
    /// It has no lock now.
    FirstCall,
    /// This task was enqueued, now it is woken for return [`Poll::Ready`],
    /// because a [`WaitSend`] has written to the slot already.
    ///
    /// # Lock note
    ///
    /// And it has no lock now.
    WokenToReturnReady,
    /// This task was enqueued, now it is woken by close.
    ///
    /// # Lock note
    ///
    /// It has no lock now.
    WokenByClose,
}

struct Inner<T> {
    storage: VecDeque<T>,
    is_closed: bool,
    capacity: usize,
    senders: VecDeque<(Task, *mut SendCallState<T>)>,
    receivers: VecDeque<(Task, *mut T, *mut RecvCallState)>,
}

unsafe impl<T> Sync for Inner<T> {}
unsafe impl<T> Send for Inner<T> {}

// region futures

macro_rules! return_pending_and_release_lock {
    ($ex:expr, $lock:expr) => {
        unsafe { $ex.release_atomic_bool($lock.leak_to_atomic()) };
        return Poll::Pending;
    };
}

macro_rules! acquire_lock {
    ($mutex:expr) => {
        match $mutex.try_lock() {
            Some(lock) => lock,
            None => {
                unsafe { local_executor().push_current_task_at_the_start_of_lifo_global_queue() };
                return Poll::Pending;
            }
        }
    };
}

pub struct WaitSend<'future, T> {
    inner: &'future NaiveMutex<Inner<T>>,
    call_state: SendCallState<T>,
    value: ManuallyDrop<T>,
    #[cfg(debug_assertions)]
    was_awaited: bool,
}

impl<'future, T> WaitSend<'future, T> {
    #[inline(always)]
    fn new(value: T, inner: &'future NaiveMutex<Inner<T>>) -> Self {
        Self {
            inner,
            call_state: SendCallState::FirstCall,
            value: ManuallyDrop::new(value),
            #[cfg(debug_assertions)]
            was_awaited: false,
        }
    }
}

impl<'future, T> Future for WaitSend<'future, T> {
    type Output = Result<(), T>;

    #[inline(always)]
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        #[cfg(debug_assertions)]
        {
            this.was_awaited = true;
        }
        panic_if_local_in_future!(cx, "Channel");

        match this.call_state {
            SendCallState::FirstCall => {
                let ex = local_executor();
                let mut inner_lock = acquire_lock!(this.inner);

                if inner_lock.is_closed {
                    return Poll::Ready(Err(unsafe { ManuallyDrop::take(&mut this.value) }));
                }

                if inner_lock.receivers.len() > 0 {
                    unsafe {
                        let (task, slot, call_state) =
                            inner_lock.receivers.pop_front().unwrap_unchecked();

                        inner_lock.unlock();

                        ptr::copy_nonoverlapping(this.value.deref(), slot, 1);
                        call_state.write(RecvCallState::WokenToReturnReady);
                        ex.exec_task(task);

                        return Poll::Ready(Ok(()));
                    }
                }

                let len = inner_lock.storage.len();
                if len >= inner_lock.capacity {
                    let task = unsafe { (cx.waker().data() as *mut Task).read() };
                    inner_lock.senders.push_back((task, &mut this.call_state));
                    return_pending_and_release_lock!(ex, inner_lock);
                }

                unsafe {
                    inner_lock
                        .storage
                        .push_back(ManuallyDrop::take(&mut this.value));
                }

                Poll::Ready(Ok(()))
            }
            SendCallState::WokenToWriteIntoQueueWithLock => {
                let inner_lock = unsafe { this.inner.get_locked() };
                unsafe {
                    inner_lock
                        .storage
                        .push_back(ManuallyDrop::take(&mut this.value));
                    this.inner.unlock();
                }
                Poll::Ready(Ok(()))
            }
            SendCallState::WokenToWriteIntoTheSlot(slot_ptr) => {
                unsafe {
                    ptr::copy_nonoverlapping(this.value.deref(), slot_ptr, 1);
                };
                Poll::Ready(Ok(()))
            }
            SendCallState::WokenByClose => {
                Poll::Ready(Err(unsafe { ManuallyDrop::take(&mut this.value) }))
            }
        }
    }
}

unsafe impl<T> Send for WaitSend<'_, T> {}

#[cfg(debug_assertions)]
impl<T> Drop for WaitSend<'_, T> {
    fn drop(&mut self) {
        assert!(
            self.was_awaited,
            "`WaitSend` was not awaited. This will cause a memory leak."
        );
    }
}

/// Will [`write`](ptr::write) the value in the `slot`. Not [`replace`](ptr::replace).
pub struct WaitRecv<'future, T> {
    inner: &'future NaiveMutex<Inner<T>>,
    call_state: RecvCallState,
    slot: *mut T,
}

impl<'future, T> WaitRecv<'future, T> {
    #[inline(always)]
    fn new(inner: &'future NaiveMutex<Inner<T>>, slot: *mut T) -> Self {
        Self {
            inner,
            call_state: RecvCallState::FirstCall,
            slot,
        }
    }
}

impl<'future, T> Future for WaitRecv<'future, T> {
    type Output = Result<(), ()>;

    #[inline(always)]
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        panic_if_local_in_future!(cx, "Channel");

        match this.call_state {
            RecvCallState::FirstCall => {
                let ex = local_executor();
                let mut inner_lock = acquire_lock!(this.inner);

                if inner_lock.is_closed {
                    return Poll::Ready(Err(()));
                }

                let l = inner_lock.storage.len();
                if l == 0 {
                    if inner_lock.senders.len() > 0 {
                        unsafe {
                            let (task, call_state) =
                                inner_lock.senders.pop_front().unwrap_unchecked();

                            inner_lock.unlock();

                            call_state.write(SendCallState::WokenToWriteIntoTheSlot(this.slot));
                            ex.exec_task(task);

                            return Poll::Ready(Ok(()));
                        }
                    }

                    let task = unsafe { (cx.waker().data() as *mut Task).read() };
                    inner_lock
                        .receivers
                        .push_back((task, this.slot, &mut this.call_state));
                    return_pending_and_release_lock!(ex, inner_lock);
                }

                unsafe {
                    this.slot
                        .write(inner_lock.storage.pop_front().unwrap_unchecked())
                }

                if inner_lock.senders.len() > 0 {
                    unsafe {
                        let (task, call_state) = inner_lock.senders.pop_front().unwrap_unchecked();
                        inner_lock.leak();
                        call_state.write(SendCallState::WokenToWriteIntoQueueWithLock);
                        ex.exec_task(task);
                    }
                }

                Poll::Ready(Ok(()))
            }

            RecvCallState::WokenToReturnReady => Poll::Ready(Ok(())),

            RecvCallState::WokenByClose => Poll::Ready(Err(())),
        }
    }
}

unsafe impl<T> Send for WaitRecv<'_, T> {}

// endregion

#[inline(always)]
async fn close<T>(inner: &NaiveMutex<Inner<T>>) {
    let mut inner_lock = inner.lock().await;
    inner_lock.is_closed = true;
    let executor = local_executor();

    for (task, call_state) in inner_lock.senders.drain(..) {
        unsafe {
            call_state.write(SendCallState::WokenByClose);
        }
        executor.spawn_global_task(task);
    }

    for (task, _, call_state) in inner_lock.receivers.drain(..) {
        unsafe {
            call_state.write(RecvCallState::WokenByClose);
        }
        executor.spawn_global_task(task);
    }
}

// region sender

pub struct Sender<'channel, T> {
    inner: &'channel NaiveMutex<Inner<T>>,
}

impl<'channel, T> Sender<'channel, T> {
    #[inline(always)]
    fn new(inner: &'channel NaiveMutex<Inner<T>>) -> Self {
        Self { inner }
    }

    #[inline(always)]
    pub fn send(&self, value: T) -> WaitSend<'_, T> {
        WaitSend::new(value, self.inner)
    }

    #[inline(always)]
    pub async fn close(&self) {
        close(self.inner).await;
    }
}

impl<'channel, T> Clone for Sender<'channel, T> {
    fn clone(&self) -> Self {
        Sender { inner: self.inner }
    }
}

unsafe impl<'channel, T> Sync for Sender<'channel, T> {}
unsafe impl<'channel, T> Send for Sender<'channel, T> {}

// endregion

// region receiver

pub struct Receiver<'channel, T> {
    inner: &'channel NaiveMutex<Inner<T>>,
}

impl<'channel, T> Receiver<'channel, T> {
    #[inline(always)]
    fn new(inner: &'channel NaiveMutex<Inner<T>>) -> Self {
        Self { inner }
    }

    #[inline(always)]
    pub async fn recv(&self) -> Result<T, ()> {
        let mut slot = MaybeUninit::uninit();
        unsafe {
            match self.recv_in_ptr(slot.as_mut_ptr()).await {
                Ok(_) => Ok(slot.assume_init()),
                Err(_) => Err(()),
            }
        }
    }

    #[inline(always)]
    /// Drops the old value in the slot.
    pub fn recv_in(&self, slot: &'channel mut T) -> WaitRecv<'channel, T> {
        unsafe { drop_in_place(slot) };
        WaitRecv::new(self.inner, slot)
    }

    #[inline(always)]
    /// Doesn't drop the old value in the slot.
    pub unsafe fn recv_in_ptr(&self, slot: *mut T) -> WaitRecv<T> {
        WaitRecv::new(self.inner, slot)
    }

    #[inline(always)]
    pub async fn close(self) {
        close(self.inner).await;
    }
}

impl<'channel, T> Clone for Receiver<'channel, T> {
    fn clone(&self) -> Self {
        Receiver { inner: self.inner }
    }
}

unsafe impl<'channel, T> Sync for Receiver<'channel, T> {}
unsafe impl<'channel, T> Send for Receiver<'channel, T> {}

// endregion

// region channel

pub struct Channel<T> {
    inner: NaiveMutex<Inner<T>>,
}

impl<T> Channel<T> {
    #[inline(always)]
    pub fn bounded(capacity: usize) -> Self {
        Self {
            inner: NaiveMutex::new(Inner {
                storage: VecDeque::with_capacity(capacity),
                capacity,
                is_closed: false,
                senders: VecDeque::with_capacity(0),
                receivers: VecDeque::with_capacity(0),
            }),
        }
    }

    #[inline(always)]
    pub fn unbounded() -> Self {
        Self {
            inner: NaiveMutex::new(Inner {
                storage: VecDeque::with_capacity(0),
                capacity: 2 << 32,
                is_closed: false,
                senders: VecDeque::with_capacity(0),
                receivers: VecDeque::with_capacity(0),
            }),
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
            match self.recv_in_ptr(slot.as_mut_ptr()).await {
                Ok(_) => Ok(slot.assume_init()),
                Err(_) => Err(()),
            }
        }
    }

    #[inline(always)]
    /// Drops the old value in the slot.
    pub fn recv_in<'future>(&'future self, slot: &'future mut T) -> WaitRecv<'future, T> {
        unsafe { drop_in_place(slot) };
        WaitRecv::new(&self.inner, slot)
    }

    #[inline(always)]
    /// Doesn't drop the old value in the slot.
    pub unsafe fn recv_in_ptr(&self, slot: *mut T) -> WaitRecv<T> {
        WaitRecv::new(&self.inner, slot)
    }

    #[inline(always)]
    pub async fn close(&self) {
        close(&self.inner).await;
    }

    #[inline(always)]
    pub fn split(&self) -> (Sender<T>, Receiver<T>) {
        (Sender::new(&self.inner), Receiver::new(&self.inner))
    }
}

unsafe impl<T> Sync for Channel<T> {}
unsafe impl<T> Send for Channel<T> {}

// endregion

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::Relaxed;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use crate::sync::channel::Channel;
    use crate::sync::WaitGroup;
    use crate::utils::droppable_element::DroppableElement;
    use crate::utils::{get_core_ids, SpinLock};
    use crate::{sleep, Executor};

    #[orengine_macros::test_global]
    fn test_zero_capacity() {
        let ch = Arc::new(Channel::bounded(0));
        let ch_clone = ch.clone();

        thread::spawn(move || {
            let ex = Executor::init();
            ex.run_with_global_future(async move {
                ch_clone.send(1).await.expect("closed");
                ch_clone.send(2).await.expect("closed");
                ch_clone.close().await;
            });
        });

        let res = ch.recv().await.expect("closed");
        assert_eq!(res, 1);

        sleep(Duration::from_millis(1)).await;

        let res = ch.recv().await.expect("closed");
        assert_eq!(res, 2);

        match ch.send(2).await {
            Err(_) => assert!(true),
            _ => panic!("should be closed"),
        };
    }

    const N: usize = 10_025;

    #[orengine_macros::test_global]
    fn test_channel() {
        let ch = Arc::new(Channel::bounded(N));
        let ch_clone = ch.clone();

        thread::spawn(move || {
            let ex = Executor::init();
            ex.run_with_global_future(async move {
                for i in 0..N {
                    ch_clone.send(i).await.expect("closed");
                }

                sleep(Duration::from_millis(1)).await;

                ch_clone.close().await;
            });
        });

        for i in 0..N {
            let res = ch.recv().await.expect("closed");
            assert_eq!(res, i);
        }

        match ch.recv().await {
            Err(_) => assert!(true),
            _ => panic!("should be closed"),
        };
    }

    #[orengine_macros::test_global]
    fn test_wait_recv() {
        let ch = Arc::new(Channel::bounded(1));
        let ch_clone = ch.clone();

        thread::spawn(move || {
            let ex = Executor::init();
            ex.run_with_global_future(async move {
                sleep(Duration::from_millis(1)).await;
                ch_clone.send(1).await.expect("closed");

                ch_clone.close().await;
            });
        });

        let res = ch.recv().await.expect("closed");
        assert_eq!(res, 1);

        match ch.recv().await {
            Err(_) => assert!(true),
            _ => panic!("should be closed"),
        };
    }

    #[orengine_macros::test_global]
    fn test_wait_send() {
        let ch = Arc::new(Channel::bounded(1));
        let ch_clone = ch.clone();

        thread::spawn(move || {
            let ex = Executor::init();
            ex.run_with_global_future(async move {
                ch_clone.send(1).await.expect("closed");
                ch_clone.send(2).await.expect("closed");

                sleep(Duration::from_millis(1)).await;

                ch_clone.close().await;
            });
        });

        sleep(Duration::from_millis(1)).await;

        let res = ch.recv().await.expect("closed");
        assert_eq!(res, 1);
        let res = ch.recv().await.expect("closed");
        assert_eq!(res, 2);

        let _ = ch.send(3).await;
        match ch.send(4).await {
            Err(_) => assert!(true),
            _ => panic!("should be closed"),
        };
    }

    #[orengine_macros::test_global]
    fn test_unbounded_channel() {
        let ch = Arc::new(Channel::unbounded());
        let wg = Arc::new(WaitGroup::new());
        let ch_clone = ch.clone();
        let wg_clone = wg.clone();

        wg.inc();
        thread::spawn(move || {
            let ex = Executor::init();
            ex.run_with_global_future(async move {
                for i in 0..N {
                    ch_clone.send(i).await.expect("closed");
                }

                let _ = wg_clone.wait().await;

                ch_clone.close().await;
            });
        });

        for i in 0..N {
            let res = ch.recv().await.expect("closed");
            assert_eq!(res, i);
        }

        wg.done();

        match ch.recv().await {
            Err(_) => assert!(true),
            _ => panic!("should be closed"),
        };
    }

    #[orengine_macros::test_global("timeout_secs=1000")]
    fn test_drop_channel() {
        let dropped = Arc::new(SpinLock::new(Vec::new()));
        let channel = Channel::bounded(1);

        let _ = channel
            .send(DroppableElement::new(1, dropped.clone()))
            .await;
        let mut prev_elem = DroppableElement::new(2, dropped.clone());
        channel.recv_in(&mut prev_elem).await.expect("closed");
        assert_eq!(prev_elem.value, 1);
        assert_eq!(dropped.lock().as_slice(), [2]);

        let _ = channel
            .send(DroppableElement::new(3, dropped.clone()))
            .await;
        unsafe { channel.recv_in_ptr(&mut prev_elem).await }.expect("closed");
        assert_eq!(prev_elem.value, 3);
        assert_eq!(dropped.lock().as_slice(), [2]);

        channel.close().await;
        let elem = channel
            .send(DroppableElement::new(5, dropped.clone()))
            .await
            .unwrap_err();
        assert_eq!(elem.value, 5);
        assert_eq!(dropped.lock().as_slice(), [2]);
    }

    #[orengine_macros::test_global]
    fn test_drop_channel_split() {
        let channel = Channel::bounded(1);
        let dropped = Arc::new(SpinLock::new(Vec::new()));
        let (sender, receiver) = channel.split();

        let _ = sender.send(DroppableElement::new(1, dropped.clone())).await;
        let mut prev_elem = DroppableElement::new(2, dropped.clone());
        receiver.recv_in(&mut prev_elem).await.expect("closed");
        assert_eq!(prev_elem.value, 1);
        assert_eq!(dropped.lock().as_slice(), [2]);

        let _ = sender.send(DroppableElement::new(3, dropped.clone())).await;
        unsafe { receiver.recv_in_ptr(&mut prev_elem).await.expect("closed") };
        assert_eq!(prev_elem.value, 3);
        assert_eq!(dropped.lock().as_slice(), [2]);

        sender.close().await;
        let elem = sender
            .send(DroppableElement::new(5, dropped.clone()))
            .await
            .unwrap_err();
        assert_eq!(elem.value, 5);
        assert_eq!(dropped.lock().as_slice(), [2]);
    }

    async fn stress_test(channel: Channel<usize>) {
        let channel = Arc::new(channel);
        let wg = Arc::new(WaitGroup::new());
        let sent = Arc::new(AtomicUsize::new(0));
        let received = Arc::new(AtomicUsize::new(0));

        for i in 0..get_core_ids().unwrap().len() * 4 {
            let channel = channel.clone();
            let wg = wg.clone();
            let sent = sent.clone();
            let received = received.clone();
            wg.add(1);

            thread::spawn(move || {
                Executor::init().run_with_global_future(async move {
                    if i % 2 == 0 {
                        for j in 0..100 {
                            channel.send(j).await.expect("closed");
                            sent.fetch_add(j, Relaxed);
                        }
                    } else {
                        for _ in 0..100 {
                            let res = channel.recv().await.expect("closed");
                            received.fetch_add(res, Relaxed);
                        }
                    }

                    wg.done();
                });
            });
        }

        let _ = wg.wait().await;
        assert_eq!(sent.load(Relaxed), received.load(Relaxed));
    }

    #[orengine_macros::test_global]
    fn stress_test_bounded_channel() {
        stress_test(Channel::bounded(1024)).await;
    }

    #[orengine_macros::test_global]
    fn stress_test_unbounded_channel() {
        stress_test(Channel::unbounded()).await;
    }

    #[orengine_macros::test_global]
    fn stress_test_zero_capacity_channel() {
        stress_test(Channel::bounded(0)).await;
    }
}
