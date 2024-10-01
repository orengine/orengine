use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::future::Future;
use std::mem::{ManuallyDrop, MaybeUninit};
use std::ops::Deref;
use std::ptr;
use std::ptr::drop_in_place;
use std::task::{Context, Poll};
use crate::runtime::{local_executor, Task};

/// This is the internal data structure for the [`local channel`](LocalChannel).
/// It holds the actual storage for the values and manages the queue of senders and receivers.
struct Inner<T> {
    storage: VecDeque<T>,
    is_closed: bool,
    capacity: usize,
    senders: VecDeque<Task>,
    receivers: VecDeque<(Task, *mut T)>
}

// region futures

/// This struct represents a future that waits for a value to be sent 
/// into the [`local channel`](LocalChannel).
/// When the future is polled, it either sends the value immediately (if there is capacity) or
/// gets parked in the list of waiting senders.
///
/// # Panics or memory leaks
///
/// If [`WaitLocalSend::poll`] is not called.
pub struct WaitLocalSend<'future, T> {
    inner: &'future mut Inner<T>,
    value: ManuallyDrop<T>,
    #[cfg(debug_assertions)]
    was_awaited: bool
}

impl<'future, T> WaitLocalSend<'future, T> {
    /// Creates a new [`WaitLocalSend`].
    #[inline(always)]
    fn new(value: T, inner: &'future mut Inner<T>) -> Self {
        Self {
            inner,
            value: ManuallyDrop::new(value),
            #[cfg(debug_assertions)]
            was_awaited: false
        }
    }
}

impl<'future, T> Future for WaitLocalSend<'future, T> {
    type Output = Result<(), T>;

    #[inline(always)]
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        #[cfg(debug_assertions)]
        { this.was_awaited = true; }

        if this.inner.is_closed {
            return Poll::Ready(Err(unsafe { ManuallyDrop::take(&mut this.value) }));
        }

        if this.inner.receivers.len() > 0 {
            let (task, slot) = unsafe {
                this.inner.receivers.pop_front().unwrap_unchecked()
            };
            unsafe { ptr::copy_nonoverlapping(this.value.deref(), slot, 1); };
            local_executor().exec_task(task);
            return Poll::Ready(Ok(()));
        }

        let len = this.inner.storage.len();
        if len >= this.inner.capacity {
            let task = unsafe { (cx.waker().data() as *mut Task).read() };
            this.inner.senders.push_back(task);
            return Poll::Pending;
        }

        unsafe {
            this.inner.storage.push_back(ManuallyDrop::take(&mut this.value));
        }
        Poll::Ready(Ok(()))
    }
}

#[cfg(debug_assertions)]
impl<'future, T> Drop for WaitLocalSend<'future, T> {
    fn drop(&mut self) {
        assert!(self.was_awaited, "WaitLocalSend was not awaited. This will cause a memory leak.");
    }
}

/// This struct represents a future that waits for a value to be 
/// received from the [`local channel`](LocalChannel).
/// When the future is polled, it either receives the value immediately (if available) or
/// gets parked in the list of waiting receivers.
pub struct WaitLocalRecv<'future, T> {
    inner: &'future mut Inner<T>,
    was_enqueued: bool,
    slot: *mut T
}

impl<'future, T> WaitLocalRecv<'future, T> {
    /// Creates a new [`WaitLocalRecv`].
    #[inline(always)]
    fn new(inner: &'future mut Inner<T>, slot: *mut T) -> Self {
        Self {
            inner,
            was_enqueued: false,
            slot
        }
    }
}

impl<'future, T> Future for WaitLocalRecv<'future, T> {
    type Output = Result<(), ()>;

    #[inline(always)]
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        if this.inner.is_closed {
            return Poll::Ready(Err(()));
        }

        if this.was_enqueued {
            return Poll::Ready(Ok(()));
        }

        if this.inner.senders.len() > 0 {
            unsafe { local_executor().spawn_local_task(this.inner.senders.pop_front().unwrap_unchecked()); }
        }

        let l = this.inner.storage.len();
        if l == 0 {
            let task = unsafe { (cx.waker().data() as *mut Task).read() };
            this.was_enqueued = true;
            this.inner.receivers.push_back((task, this.slot));
            return Poll::Pending;
        }

        Poll::Ready(Ok(unsafe {
            ptr::write(this.slot, this.inner.storage.pop_front().unwrap_unchecked())
        }))
    }
}

// endregion

/// Closes the [`local channel`](LocalChannel) and wakes all senders and receivers.
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

/// The `LocalSender` allows sending values into the [`LocalChannel`].
/// When the [`local channel`](LocalChannel) is not full, values are sent immediately.
/// If the [`local channel`](LocalChannel) is full, the sender waits until capacity 
/// is available or the [`local channel`](LocalChannel) is closed.
///
/// # Example
///
/// ```no_run
/// async fn foo() {
///     let channel = orengine::sync::local::LocalChannel::bounded(2); // capacity = 2
///     let (sender, receiver) = channel.split();
///
///     sender.send(1).await.unwrap();
///     let res = receiver.recv().await.unwrap();
///     assert_eq!(res, 1);
/// }
/// ```
pub struct LocalSender<'channel, T> {
    inner: &'channel UnsafeCell<Inner<T>>,
    // impl !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

impl<'channel, T> LocalSender<'channel, T> {
    /// Creates a new [`LocalSender`].
    #[inline(always)]
    fn new(inner: &'channel UnsafeCell<Inner<T>>) -> Self {
        Self {
            inner,
            no_send_marker: std::marker::PhantomData
        }
    }

    /// Sends a value into the [`local channel`](LocalChannel).
    ///
    /// # On close
    ///
    /// Returns `Err(T)` if the [`local channel`](LocalChannel) is closed.
    /// 
    /// # Example
    ///
    /// ```no_run
    /// async fn foo() {
    ///     let channel = orengine::sync::local::LocalChannel::bounded(1); // capacity = 1
    ///     let (sender, receiver) = channel.split();
    ///
    ///     sender.send(1).await.unwrap(); // not blocked
    ///     sender.send(2).await.unwrap(); // blocked forever, because recv will never be called
    /// }
    /// ```
    #[inline(always)]
    pub fn send(&self, value: T) -> WaitLocalSend<'_, T> {
        WaitLocalSend::new(value, unsafe { &mut * self.inner.get() })
    }

    /// Closes the [`LocalChannel`] associated with this sender.
    #[inline(always)]
    pub fn close(&self) {
        let inner = unsafe { &mut * self.inner.get() };
        close(inner);
    }
}

impl<'channel, T> Clone for LocalSender<'channel, T> {
    fn clone(&self) -> Self {
        LocalSender {
            inner: self.inner,
            no_send_marker: std::marker::PhantomData
        }
    }
}

unsafe impl<'channel, T> Sync for LocalSender<'channel, T> {}

// endregion

// region receiver

/// The `LocalReceiver` allows receiving values from the [`LocalChannel`].
/// When the [`local channel`](LocalChannel) is not empty, values are received immediately.
/// If the [`local channel`](LocalChannel) is empty, the receiver waits until a value 
/// is available or the [`local channel`](LocalChannel) is closed.
///
/// # Example
///
/// ```no_run
/// async fn foo() {
///     let channel = orengine::sync::local::LocalChannel::bounded(2); // capacity = 2
///     let (sender, receiver) = channel.split();
///
///     sender.send(1).await.unwrap();
///     let res = receiver.recv().await.unwrap();
///     assert_eq!(res, 1);
/// }
///
pub struct LocalReceiver<'channel, T> {
    inner: &'channel UnsafeCell<Inner<T>>,
    // impl !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

impl<'channel, T> LocalReceiver<'channel, T> {
    /// Creates a new [`LocalReceiver`].
    #[inline(always)]
    fn new(inner: &'channel UnsafeCell<Inner<T>>) -> Self {
        Self {
            inner,
            no_send_marker: std::marker::PhantomData
        }
    }

    /// Asynchronously receives a value from the [`channel`](LocalChannel).
    ///
    /// If the [`local channel`](LocalChannel) is empty, the receiver waits until a value 
    /// is available  or the [`local channel`](LocalChannel) is closed.
    ///
    /// Else, the value is immediately received.
    ///
    /// # On close
    ///
    /// Returns `Err(())` if the [`local channel`](LocalChannel) is closed.
    /// 
    /// # Example
    /// 
    /// ```no_run
    /// async fn foo() {
    ///     let channel = orengine::sync::local::LocalChannel::bounded(1); // capacity = 1
    ///     let (sender, receiver) = channel.split();
    ///
    ///     sender.send(1).await.unwrap();
    ///     let res = receiver.recv().await.unwrap(); // not blocked
    ///     assert_eq!(res, 1);
    ///     let _ = receiver.recv().await.unwrap(); // blocked forever because send will never be called
    /// }
    /// ```
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

    /// Asynchronously receives a value from the [`channel`](LocalChannel) to the provided slot.
    ///
    /// If the [`local channel`](LocalChannel) is empty, the receiver waits until a value 
    /// is available or the [`local channel`](LocalChannel) is closed.
    ///
    /// Else, the value is immediately received.
    ///
    /// # On close
    ///
    /// Returns `Err(())` if the [`local channel`](LocalChannel) is closed.
    ///
    /// # Attention
    ///
    /// `Drops` the old value in the slot.
    ///
    /// # Example
    ///
    /// ```no_run
    /// async fn foo() {
    ///     let channel = orengine::sync::local::LocalChannel::bounded(1); // capacity = 1
    ///     let (sender, receiver) = channel.split();
    ///
    ///     sender.send(1).await.unwrap();
    ///     let mut res = receiver.recv().await.unwrap(); // not blocked
    ///     assert_eq!(res, 1);
    ///     receiver.recv_in(&mut res).await.unwrap(); // blocked forever because send will never be called
    /// }
    /// ```
    #[inline(always)]
    pub fn recv_in<'future>(&self, slot: &'future mut T) -> WaitLocalRecv<'future, T> {
        unsafe { drop_in_place(slot) };
        WaitLocalRecv::new(unsafe { &mut * self.inner.get() }, slot)
    }
    
    /// Asynchronously receives a value from the [`channel`](LocalChannel) to the provided slot.
    ///
    /// If the [`local channel`](LocalChannel) is empty, the receiver waits until a value
    /// is available or the [`local channel`](LocalChannel) is closed.
    ///
    /// Else, the value is immediately received.
    ///
    /// # On close
    ///
    /// Returns `Err(())` if the [`local channel`](LocalChannel) is closed.
    ///
    /// # Attention
    ///
    /// `Doesn't drop` the old value in the slot.
    ///
    /// # Example
    ///
    /// ```no_run
    /// async fn foo() {
    ///     let channel = orengine::sync::local::LocalChannel::bounded(1); // capacity = 1
    ///     let (sender, receiver) = channel.split();
    ///
    ///     sender.send(1).await.unwrap();
    ///     let mut res_: std::mem::MaybeUninit<usize> = std::mem::MaybeUninit::uninit();
    ///     unsafe { receiver.recv_in_ptr(res_.as_mut_ptr()).await.unwrap(); } // not blocked
    ///     let res = unsafe { res_.assume_init() }; // 1
    ///     unsafe { 
    ///         receiver.recv_in_ptr(res_.as_mut_ptr()).await.unwrap(); // blocked forever because send will never be called
    ///     }
    /// }
    /// ```
    #[inline(always)]
    pub unsafe fn recv_in_ptr<'future>(&self, slot: *mut T) -> WaitLocalRecv<'future, T> {
        WaitLocalRecv::new(unsafe { &mut * self.inner.get() }, slot)
    }

    /// Closes the [`LocalChannel`] associated with this receiver.
    #[inline(always)]
    pub fn close(self) {
        let inner = unsafe { &mut * self.inner.get() };
        close(inner);
    }
}

impl<'channel, T> Clone for LocalReceiver<'channel, T> {
    fn clone(&self) -> Self {
        LocalReceiver {
            inner: self.inner,
            no_send_marker: std::marker::PhantomData
        }
    }
}

unsafe impl<'channel, T> Sync for LocalReceiver<'channel, T> {}

// endregion

// region channel

/// The `LocalChannel` provides an asynchronous communication channel between 
/// tasks running on the same thread. 
/// 
/// It supports both [`bounded`](LocalChannel::bounded) and [`unbounded`](LocalChannel::unbounded)
/// channels for sending and receiving values.
///
/// When the [`local channel`](LocalChannel) is not empty, values are received immediately else 
/// the reception operation is waiting until a value is available or
/// the [`local channel`](LocalChannel) is closed.
/// 
/// When channel is not full, values are sent immediately else 
/// the sending operation is waiting until capacity is available or 
/// the [`local channel`](LocalChannel) is closed.
/// 
/// # The difference between `LocalChannel` and [`Channel`](crate::sync::Channel)
/// 
/// The `LocalChannel` works with `local tasks`. 
/// 
/// Read [`Executor`](crate::Executor) for more details.
///
/// # Examples
/// 
/// ## Don't split
/// 
/// ```no_run
/// async fn foo() {
///     let channel = orengine::sync::local::LocalChannel::bounded(1); // capacity = 1
///
///     channel.send(1).await.unwrap();
///     let res = channel.recv().await.unwrap();
///     assert_eq!(res, 1);
/// }
/// ```
/// 
/// ## Split into receiver and sender
/// 
/// ```no_run
/// async fn foo() {
///     let channel = orengine::sync::local::LocalChannel::bounded(1); // capacity = 1
///     let (sender, receiver) = channel.split();
///
///     sender.send(1).await.unwrap();
///     let res = receiver.recv().await.unwrap();
///     assert_eq!(res, 1);
/// }
/// ```
pub struct LocalChannel<T> {
    inner: UnsafeCell<Inner<T>>,
    // impl !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

impl<T> LocalChannel<T> {
    /// Creates a bounded channel with a given capacity.
    ///
    /// A bounded channel limits the number of items that can be stored before sending blocks.
    /// Once the [`local channel`](LocalChannel) reaches its capacity, 
    /// senders will block until space becomes available.
    ///
    /// # Example
    ///
    /// ```no_run
    /// async fn foo() {
    ///     let channel = orengine::sync::local::LocalChannel::bounded(1);
    ///
    ///     channel.send(1).await.unwrap(); // not blocked
    ///     channel.send(2).await.unwrap(); // blocked because the [`local channel`](LocalChannel) is full
    /// }
    /// ```
    #[inline(always)]
    pub fn bounded(capacity: usize) -> Self {
        Self {
            inner: UnsafeCell::new(Inner {
                storage: VecDeque::with_capacity(capacity),
                capacity,
                is_closed: false,
                senders: VecDeque::with_capacity(0),
                receivers: VecDeque::with_capacity(0)
            }),
            no_send_marker: std::marker::PhantomData
        }
    }

    /// Creates an unbounded channel.
    ///
    /// An unbounded channel allows senders to send an unlimited number of values.
    ///
    /// # Example
    ///
    /// ```no_run
    /// async fn foo() {
    ///     let channel = orengine::sync::local::LocalChannel::unbounded();
    ///
    ///     channel.send(1).await.unwrap(); // not blocked
    ///     channel.send(2).await.unwrap(); // not blocked
    /// }
    /// ```
    #[inline(always)]
    pub fn unbounded() -> Self {
        Self {
            inner: UnsafeCell::new(Inner {
                storage: VecDeque::with_capacity(0),
                capacity: 2 << 32,
                is_closed: false,
                senders: VecDeque::with_capacity(0),
                receivers: VecDeque::with_capacity(0)
            }),
            no_send_marker: std::marker::PhantomData
        }
    }

    /// Sends a value into the [`local channel`](LocalChannel), returning a [`WaitLocalSend`] future.
    ///
    /// This method is usually called by the `LocalSender`. It does not block the sender directly;
    /// instead, it returns a future that will complete once the value 
    /// is sent or the [`local channel`](LocalChannel) is closed.
    /// 
    /// # On close
    /// 
    /// Returns `Err(T)` if the [`local channel`](LocalChannel) is closed.
    ///
    /// # Panics or memory leaks
    ///
    /// If the future is not polled (awaited).
    ///
    /// # Example
    ///
    /// ```no_run
    /// async fn foo() {
    ///     let channel = orengine::sync::local::LocalChannel::bounded(0);
    ///     channel.send(1).await.unwrap(); // blocked
    /// }
    /// ```
    #[inline(always)]
    pub fn send(&self, value: T) -> WaitLocalSend<'_, T> {
        WaitLocalSend::new(value, unsafe { &mut * self.inner.get() })
    }

    /// Receives a value from the [`local channel`](LocalChannel), returning a future 
    /// that completes with the result.
    ///
    /// If the [`local channel`](LocalChannel) is empty, the receiver waits until a value is available
    /// or the [`local channel`](LocalChannel) is closed.
    /// 
    /// # On close
    /// 
    /// Returns `Err(())` if the [`local channel`](LocalChannel) is closed.
    ///
    /// # Example
    ///
    /// ```no_run
    /// async fn foo() {
    ///     let channel = orengine::sync::local::LocalChannel::<usize>::bounded(1);
    ///     let res = channel.recv().await.unwrap(); // blocked until a value is sent
    /// }
    /// ```
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

    /// Asynchronously receives a value from the `LocalChannel` to the provided slot.
    ///
    /// If the [`local channel`](LocalChannel) is empty, the receiver waits 
    /// until a value is available
    /// or the [`local channel`](LocalChannel) is closed.
    ///
    /// Else, the value is immediately received.
    ///
    /// # On close
    ///
    /// Returns `Err(())` if the [`local channel`](LocalChannel) is closed.
    ///
    /// # Attention
    ///
    /// `Drops` the old value in the slot.
    ///
    /// # Example
    ///
    /// ```no_run
    /// async fn foo() {
    ///     let channel = orengine::sync::local::LocalChannel::bounded(1); 
    ///
    ///     channel.send(1).await.unwrap();
    ///     let mut res = channel.recv().await.unwrap(); // not blocked
    ///     assert_eq!(res, 1);
    ///     channel.recv_in(&mut res).await.unwrap(); // blocked forever because send will never be called
    /// }
    /// ```
    #[inline(always)]
    pub fn recv_in<'future>(&self, slot: &'future mut T) -> WaitLocalRecv<'future, T> {
        unsafe { drop_in_place(slot) };
        WaitLocalRecv::new(unsafe { &mut * self.inner.get() }, slot)
    }

    /// Asynchronously receives a value from the `LocalChannel` to the provided slot.
    ///
    /// If the [`local channel`](LocalChannel) is empty, the receiver waits until 
    /// a value is available or the [`local channel`](LocalChannel) is closed.
    ///
    /// Else, the value is immediately received.
    ///
    /// # On close
    ///
    /// Returns `Err(())` if the [`local channel`](LocalChannel) is closed.
    ///
    /// # Attention
    ///
    /// `Doesn't drop` the old value in the slot.
    ///
    /// # Example
    ///
    /// ```no_run
    /// async fn foo() {
    ///     let channel = orengine::sync::local::LocalChannel::bounded(1); // capacity = 1
    ///
    ///     channel.send(1).await.unwrap();
    ///     let mut res_: std::mem::MaybeUninit<usize> = std::mem::MaybeUninit::uninit();
    ///     unsafe { channel.recv_in_ptr(res_.as_mut_ptr()).await.unwrap(); } // not blocked
    ///     let res = unsafe { res_.assume_init() }; // 1
    ///     unsafe { 
    ///         channel.recv_in_ptr(res_.as_mut_ptr()).await.unwrap(); // blocked forever because send will never be called
    ///     }
    /// }
    /// ```
    #[inline(always)]
    pub unsafe fn recv_in_ptr<'future>(&self, slot: *mut T) -> WaitLocalRecv<'future, T> {
        WaitLocalRecv::new(unsafe { &mut * self.inner.get() }, slot as _)
    }

    /// Closes the `LocalChannel`. It will wake all waiting receivers and senders.
    #[inline(always)]
    pub fn close(&self) {
        let inner = unsafe { &mut * self.inner.get() };
        close(inner);
    }

    /// Splits the [`local channel`](LocalChannel) into a [`LocalSender`] and a [`LocalReceiver`], 
    /// allowing separate sending and receiving tasks.
    ///
    /// # Example
    ///
    /// ```no_run
    /// async fn foo() {
    ///     let channel = orengine::sync::local::LocalChannel::bounded(1);
    ///     let (sender, receiver) = channel.split();
    ///
    ///     sender.send(1).await.unwrap();
    ///     let res = receiver.recv().await.unwrap();
    ///     assert_eq!(res, 1);
    /// }
    /// ```
    #[inline(always)]
    pub fn split(&self) -> (LocalSender<T>, LocalReceiver<T>) {
        (LocalSender::new(&self.inner), LocalReceiver::new(&self.inner))
    }
}

unsafe impl<T> Sync for LocalChannel<T> {}

// endregion

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use crate::sync::{local_scope, LocalWaitGroup};
    use crate::local_yield_now;
    use crate::utils::droppable_element::DroppableElement;
    use crate::utils::SpinLock;
    use super::*;

    #[orengine_macros::test]
    fn test_zero_capacity() {
        let ch = LocalChannel::bounded(0);
        let ch_ref = &ch;

        local_scope(|scope| async {
            scope.spawn(async move{
                ch_ref.send(1).await.expect("closed");

                local_yield_now().await;

                ch_ref.send(2).await.expect("closed");
                ch_ref.close();
            });

            let res = ch.recv().await.expect("closed");
            assert_eq!(res, 1);
            let res = ch.recv().await.expect("closed");
            assert_eq!(res, 2);

            match ch.send(2).await {
                Err(_) => assert!(true),
                _ => panic!("should be closed")
            };
        }).await;
    }

    #[orengine_macros::test]
    fn test_unbounded() {
        let ch = LocalChannel::unbounded();
        let ch_ref = &ch;

        local_scope(|scope| async {
            scope.spawn(async move{
                ch_ref.send(1).await.expect("closed");

                local_yield_now().await;

                for i in 2..100 {
                    ch_ref.send(i).await.expect("closed");
                }
                ch_ref.close();
            });

            for i in 1..100 {
                let res = ch.recv().await.expect("closed");
                assert_eq!(res, i);
            }

            match ch.recv().await {
                Err(_) => assert!(true),
                _ => panic!("should be closed")
            };
        }).await;
    }

    const N: usize = 10_025;

    // case 1 - send N and recv N. No wait
    // case 2 - send N and recv (N + 1). Wait for recv
    // case 3 - send (N + 1) and recv N. Wait for send
    // case 4 - send (N + 1) and recv (N + 1). Wait for send and wait for recv

    #[orengine_macros::test]
    fn test_local_channel_case1() {
        let ch = LocalChannel::bounded(N);
        let ch_ref = &ch;

        local_scope(|scope| async {
            scope.spawn(async move {
                for i in 0..N {
                    ch_ref.send(i).await.expect("closed");
                }

                local_yield_now().await;

                ch_ref.close();
            });

            for i in 0..N {
                let res = ch.recv().await.expect("closed");
                assert_eq!(res, i);
            }

            match ch.recv().await {
                Err(_) => assert!(true),
                _ => panic!("should be closed")
            };
        }).await;
    }

    #[orengine_macros::test]
    fn test_local_channel_case2() {
        let ch = LocalChannel::bounded(N);
        let ch_ref = &ch;

        local_scope(|scope| async {
            scope.spawn(async move {
                for i in 0..=N {
                    let res = ch_ref.recv().await.expect("closed");
                    assert_eq!(res, i);
                }

                ch_ref.close();
            });

            for i in 0..N {
                let _ = ch.send(i).await.expect("closed");
            }

            local_yield_now().await;

            let _ = ch.send(N).await.expect("closed");
        }).await;
    }

    #[orengine_macros::test]
    fn test_local_channel_case3() {
        let ch = LocalChannel::bounded(N);
        let ch_ref = &ch;

        local_scope(|scope| async {
            scope.spawn(async move {
                for i in 0..N {
                    let res = ch_ref.recv().await.expect("closed");
                    assert_eq!(res, i);
                }

                local_yield_now().await;

                let res = ch_ref.recv().await.expect("closed");
                assert_eq!(res, N);
            });

            for i in 0..=N {
                ch.send(i).await.expect("closed");
            }
        }).await;
    }

    #[orengine_macros::test]
    fn test_local_channel_case4() {
        let ch = LocalChannel::bounded(N);
        let ch_ref = &ch;

        local_scope(|scope| async {
            scope.spawn(async move {
                for i in 0..=N {
                    let res = ch_ref.recv().await.expect("closed");
                    assert_eq!(res, i);
                }
            });

            for i in 0..=N {
                ch.send(i).await.expect("closed");
            }
        }).await;
    }

    #[orengine_macros::test]
    fn test_local_channel_split() {
        let ch = LocalChannel::bounded(N);
        let (tx, rx) = ch.split();
        let tx_ref = &tx;

        local_scope(|scope| async {
            scope.spawn(async move {
                for i in 0..=N*2 {
                    let res = rx.recv().await.expect("closed");
                    assert_eq!(res, i);
                }
            });

            let wg = LocalWaitGroup::new();
            wg.add(1);

            scope.spawn(async {
                for i in 0..N {
                    tx_ref.send(i).await.expect("closed");
                }

                wg.done();
            });

            let _ = wg.wait().await;

            for i in N..=N*2 {
                tx.send(i).await.expect("closed");
            }
        }).await;
    }

    #[orengine_macros::test]
    fn test_drop_channel() {
        let dropped = Arc::new(SpinLock::new(Vec::new()));
        let channel = LocalChannel::bounded(1);

        let _ = channel.send(DroppableElement::new(1, dropped.clone())).await;
        let mut prev_elem = DroppableElement::new(2, dropped.clone());
        channel.recv_in(&mut prev_elem).await.expect("closed");
        assert_eq!(prev_elem.value, 1);
        assert_eq!(dropped.lock().as_slice(), [2]);

        let _ = channel.send(DroppableElement::new(3, dropped.clone())).await;
        unsafe { channel.recv_in_ptr(&mut prev_elem).await.expect("closed") };
        assert_eq!(prev_elem.value, 3);
        assert_eq!(dropped.lock().as_slice(), [2]);

        channel.close();
        let elem = channel.send(DroppableElement::new(5, dropped.clone()))
            .await
            .unwrap_err();
        assert_eq!(elem.value, 5);
        assert_eq!(dropped.lock().as_slice(), [2]);
    }

    #[orengine_macros::test]
    fn test_drop_channel_split() {
        let channel = LocalChannel::bounded(1);
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

        sender.close();
        let elem = sender.send(DroppableElement::new(5, dropped.clone()))
            .await
            .unwrap_err();
        assert_eq!(elem.value, 5);
        assert_eq!(dropped.lock().as_slice(), [2]);
    }
}