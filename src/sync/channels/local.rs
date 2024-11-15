use crate::get_task_from_context;
use crate::runtime::{local_executor, Task};
use crate::sync::{AsyncChannel, AsyncReceiver, AsyncSender, RecvInResult, SendResult, TryRecvInResult, TrySendResult};
use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::future::Future;
use std::mem::ManuallyDrop;
use std::ptr;
use std::task::{Context, Poll};

/// `SendCallState` is a state machine for [`WaitLocalSend`]. This is used to improve performance.
enum SendCallState {
    /// Default state.
    FirstCall,
    /// Receiver writes the value associated with this task
    WokenToReturnReady,
    /// This task was enqueued, now it is woken by close, and it has no lock now.
    WokenByClose,
}

/// `RecvCallState` is a state machine for [`WaitLocalRecv`]. This is used to improve performance.
enum RecvCallState {
    /// Default state.
    FirstCall,
    /// This task was enqueued, now it is woken for return [`Poll::Ready`],
    /// because a [`WaitLocalSend`] has written to the slot already.
    WokenToReturnReady,
    /// This task was enqueued, now it is woken by close.
    WokenByClose,
}

/// This is the internal data structure for the [`local channel`](LocalChannel).
/// It holds the actual storage for the values and manages the queue of senders and receivers.
struct Inner<T> {
    storage: VecDeque<T>,
    is_closed: bool,
    capacity: usize,
    senders: VecDeque<(Task, *mut SendCallState, *const T)>,
    receivers: VecDeque<(Task, *mut T, *mut RecvCallState)>,
}

// region futures

/// This struct represents a future that waits for a value to be sent
/// into the [`local channel`](LocalChannel).
///
/// When the future is polled, it either sends the value immediately (if there is capacity) or
/// gets parked in the list of waiting senders.
///
/// # Panics or memory leaks
///
/// If [`WaitLocalSend::poll`] is not called.
pub struct WaitLocalSend<'future, T> {
    inner: &'future mut Inner<T>,
    call_state: SendCallState,
    value: ManuallyDrop<T>,
    #[cfg(debug_assertions)]
    was_awaited: bool,
}

impl<'future, T> WaitLocalSend<'future, T> {
    /// Creates a new [`WaitLocalSend`].
    #[inline(always)]
    fn new(value: T, inner: &'future mut Inner<T>) -> Self {
        Self {
            inner,
            call_state: SendCallState::FirstCall,
            value: ManuallyDrop::new(value),
            #[cfg(debug_assertions)]
            was_awaited: false,
        }
    }
}

impl<'future, T> Future for WaitLocalSend<'future, T> {
    type Output = SendResult<T>;

    #[inline(always)]
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        #[cfg(debug_assertions)]
        {
            this.was_awaited = true;
        }

        match this.call_state {
            SendCallState::FirstCall => {
                if this.inner.is_closed {
                    return Poll::Ready(SendResult::Closed(unsafe { ManuallyDrop::take(&mut this.value) }));
                }

                if !this.inner.receivers.is_empty() {
                    let (task, slot, call_state) =
                        unsafe { this.inner.receivers.pop_front().unwrap_unchecked() };
                    unsafe {
                        ptr::copy_nonoverlapping(&*this.value, slot, 1);
                        call_state.write(RecvCallState::WokenToReturnReady);
                    };
                    local_executor().exec_task(task);
                    return Poll::Ready(SendResult::Ok);
                }

                let len = this.inner.storage.len();
                if len >= this.inner.capacity {
                    let task = unsafe { get_task_from_context!(cx) };
                    this.inner.senders.push_back((
                        task,
                        &mut this.call_state,
                        ptr::from_ref(&this.value).cast()
                    ));
                    return Poll::Pending;
                }

                unsafe {
                    this.inner
                        .storage
                        .push_back(ManuallyDrop::take(&mut this.value));
                }
                Poll::Ready(SendResult::Ok)
            }
            SendCallState::WokenToReturnReady => {
                Poll::Ready(SendResult::Ok)
            }
            SendCallState::WokenByClose => {
                Poll::Ready(SendResult::Closed(unsafe { ManuallyDrop::take(&mut this.value) }))
            }
        }
    }
}

#[cfg(debug_assertions)]
impl<'future, T> Drop for WaitLocalSend<'future, T> {
    fn drop(&mut self) {
        assert!(
            self.was_awaited,
            "WaitLocalSend was not awaited. This will cause a memory leak."
        );
    }
}

/// This struct represents a future that waits for a value to be
/// received from the [`local channel`](LocalChannel).
///
/// When the future is polled, it either receives the value immediately (if available) or
/// gets parked in the list of waiting receivers.
pub struct WaitLocalRecv<'future, T> {
    inner: &'future mut Inner<T>,
    call_state: RecvCallState,
    slot: *mut T,
}

impl<'future, T> WaitLocalRecv<'future, T> {
    /// Creates a new [`WaitLocalRecv`].
    #[inline(always)]
    fn new(inner: &'future mut Inner<T>, slot: *mut T) -> Self {
        Self {
            inner,
            call_state: RecvCallState::FirstCall,
            slot,
        }
    }
}

impl<'future, T> Future for WaitLocalRecv<'future, T> {
    type Output = RecvInResult;

    #[inline(always)]
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        match this.call_state {
            RecvCallState::FirstCall => {
                if this.inner.is_closed {
                    return Poll::Ready(RecvInResult::Closed);
                }

                let l = this.inner.storage.len();
                if l == 0 {
                    let sender_ = this.inner.senders.pop_front();
                    if let Some((task, call_state, value_ptr)) = sender_ {
                        unsafe {
                            call_state.write(SendCallState::WokenToReturnReady);
                            ptr::copy_nonoverlapping(value_ptr, this.slot, 1);
                            local_executor().exec_task(task);

                            return Poll::Ready(RecvInResult::Ok);
                        }
                    }

                    let task = unsafe { get_task_from_context!(cx) };
                    this.inner
                        .receivers
                        .push_back((task, this.slot, &mut this.call_state));
                    return Poll::Pending;
                }

                unsafe {
                    this.slot.write(this.inner.storage.pop_front().unwrap_unchecked());
                }

                let sender_ = this.inner.senders.pop_front();
                if let Some((task, call_state, value)) = sender_ {
                    unsafe {
                        call_state.write(SendCallState::WokenToReturnReady);
                        this.inner.storage.push_back(ptr::read(value));
                        local_executor().exec_task(task);
                    }
                }

                Poll::Ready(RecvInResult::Ok)
            }

            RecvCallState::WokenToReturnReady => Poll::Ready(RecvInResult::Ok),

            RecvCallState::WokenByClose => Poll::Ready(RecvInResult::Closed),
        }
    }
}

// endregion

/// Closes the [`local channel`](LocalChannel) and wakes all senders and receivers.
#[inline(always)]
fn close<T>(inner: &mut Inner<T>) {
    inner.is_closed = true;
    let executor = local_executor();

    for (task, state_ptr, _) in inner.senders.drain(..) {
        unsafe { state_ptr.write(SendCallState::WokenByClose); };
        executor.exec_task(task);
    }

    for (task, _, state_ptr) in inner.receivers.drain(..) {
        unsafe {
            state_ptr.write(RecvCallState::WokenByClose);
        }
        executor.exec_task(task);
    }
}

macro_rules! generate_try_send {
    () => {
        fn try_send(&self, value: T) -> TrySendResult<T> {
            let inner = unsafe { &mut *self.inner.get() };
            if inner.is_closed {
                return TrySendResult::Closed(value);
            }

            if let Some((
                            task,
                            slot,
                            call_state
                        )) = inner.receivers.pop_front() {
                unsafe {
                    slot.write(value);
                    call_state.write(RecvCallState::WokenToReturnReady);
                };
                local_executor().exec_task(task);
                return TrySendResult::Ok;
            }

            let len = inner.storage.len();
            if len >= inner.capacity {
                return TrySendResult::Full(value);
            }

            inner.storage.push_back(value);

            TrySendResult::Ok
        }
    };
}

// region sender

/// The `LocalSender` allows sending values into the [`LocalChannel`].
///
/// When the [`local channel`](LocalChannel) is not full, values are sent immediately.
///
/// If the [`local channel`](LocalChannel) is full, the sender waits until capacity
/// is available or the [`local channel`](LocalChannel) is closed.
///
/// # Example
///
/// ```rust
/// use orengine::sync::{AsyncChannel, AsyncReceiver, AsyncSender};
///
/// async fn foo() {
///     let channel = orengine::sync::LocalChannel::bounded(2); // capacity = 2
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
            no_send_marker: std::marker::PhantomData,
        }
    }
}

impl<'channel, T> AsyncSender<T> for LocalSender<'channel, T> {
    fn send(&self, value: T) -> impl Future<Output=SendResult<T>> {
        WaitLocalSend::new(value, unsafe { &mut *self.inner.get() })
    }

    generate_try_send!();

    async fn sender_close(&self) {
        let inner = unsafe { &mut *self.inner.get() };
        close(inner);
    }
}

impl<'channel, T> Clone for LocalSender<'channel, T> {
    fn clone(&self) -> Self {
        LocalSender {
            inner: self.inner,
            no_send_marker: std::marker::PhantomData,
        }
    }
}

unsafe impl<'channel, T> Sync for LocalSender<'channel, T> {}

// endregion

// region receiver

/// The `LocalReceiver` allows receiving values from the [`LocalChannel`].
///
/// When the [`local channel`](LocalChannel) is not empty, values are received immediately.
///
/// If the [`local channel`](LocalChannel) is empty, the receiver waits until a value
/// is available or the [`local channel`](LocalChannel) is closed.
///
/// # Example
///
/// ```rust
/// use orengine::sync::{AsyncChannel, AsyncReceiver, AsyncSender};
///
/// async fn foo() {
///     let channel = orengine::sync::LocalChannel::bounded(2); // capacity = 2
///     let (sender, receiver) = channel.split();
///
///     sender.send(1).await.unwrap();
///     let res = receiver.recv().await.unwrap();
///     assert_eq!(res, 1);
/// }
/// ```
pub struct LocalReceiver<'channel, T> {
    inner: &'channel UnsafeCell<Inner<T>>,
    // impl !Send
    no_send_marker: std::marker::PhantomData<*const ()>,
}

macro_rules! generate_try_recv_in_ptr {
    () => {
        unsafe fn try_recv_in_ptr(&self, slot: *mut T) -> TryRecvInResult {
            let inner = unsafe { &mut *self.inner.get() };
            if inner.is_closed {
                return TryRecvInResult::Closed;
            }

            let l = inner.storage.len();
            if l == 0 {
                let sender_ = inner.senders.pop_front();
                if let Some((task, call_state, value_ptr)) = sender_ {
                    unsafe {
                        call_state.write(SendCallState::WokenToReturnReady);
                        ptr::copy_nonoverlapping(value_ptr, slot, 1);
                        local_executor().exec_task(task);

                        return TryRecvInResult::Ok;
                    }
                }

                return TryRecvInResult::Empty;
            }

            unsafe { slot.write(inner.storage.pop_front().unwrap_unchecked()); };

            let sender_ = inner.senders.pop_front();
            if let Some((task, call_state, value)) = sender_ {
                unsafe {
                    call_state.write(SendCallState::WokenToReturnReady);
                    inner.storage.push_back(ptr::read(value));
                    local_executor().exec_task(task);
                }
            }

            TryRecvInResult::Ok
        }
    };
}

impl<'channel, T> LocalReceiver<'channel, T> {
    /// Creates a new [`LocalReceiver`].
    #[inline(always)]
    fn new(inner: &'channel UnsafeCell<Inner<T>>) -> Self {
        Self {
            inner,
            no_send_marker: std::marker::PhantomData,
        }
    }
}

impl<'channel, T> AsyncReceiver<T> for LocalReceiver<'channel, T> {
    unsafe fn recv_in_ptr(&self, slot: *mut T) -> impl Future<Output=RecvInResult> {
        WaitLocalRecv::new(unsafe { &mut *self.inner.get() }, slot)
    }

    generate_try_recv_in_ptr!();

    async fn receiver_close(&self) {
        let inner = unsafe { &mut *self.inner.get() };
        close(inner);
    }
}

impl<'channel, T> Clone for LocalReceiver<'channel, T> {
    fn clone(&self) -> Self {
        LocalReceiver {
            inner: self.inner,
            no_send_marker: std::marker::PhantomData,
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
/// ```rust
/// use orengine::sync::{AsyncChannel, AsyncReceiver, AsyncSender};
///
/// async fn foo() {
///     let channel = orengine::sync::LocalChannel::bounded(1); // capacity = 1
///
///     channel.send(1).await.unwrap();
///     let res = channel.recv().await.unwrap();
///     assert_eq!(res, 1);
/// }
/// ```
///
/// ## Split into receiver and sender
///
/// ```rust
/// use orengine::sync::{AsyncChannel, AsyncReceiver, AsyncSender};
///
/// async fn foo() {
///     let channel = orengine::sync::LocalChannel::bounded(1); // capacity = 1
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

impl<T> AsyncChannel<T> for LocalChannel<T> {
    type Receiver<'channel> = LocalReceiver<'channel, T>
    where
        Self: 'channel;
    type Sender<'channel> = LocalSender<'channel, T>
    where
        Self: 'channel;

    #[inline(always)]
    fn bounded(capacity: usize) -> Self {
        Self {
            inner: UnsafeCell::new(Inner {
                storage: VecDeque::with_capacity(capacity),
                capacity,
                is_closed: false,
                senders: VecDeque::with_capacity(0),
                receivers: VecDeque::with_capacity(0),
            }),
            no_send_marker: std::marker::PhantomData,
        }
    }

    #[inline(always)]
    fn unbounded() -> Self {
        Self {
            inner: UnsafeCell::new(Inner {
                storage: VecDeque::with_capacity(0),
                capacity: usize::MAX,
                is_closed: false,
                senders: VecDeque::with_capacity(0),
                receivers: VecDeque::with_capacity(0),
            }),
            no_send_marker: std::marker::PhantomData,
        }
    }

    #[inline(always)]
    fn split(&self) -> (Self::Sender<'_>, Self::Receiver<'_>) {
        (
            LocalSender::new(&self.inner),
            LocalReceiver::new(&self.inner),
        )
    }

    async fn close(&self) {
        let inner = unsafe { &mut *self.inner.get() };
        close(inner);
    }
}

impl<T> AsyncReceiver<T> for LocalChannel<T> {
    unsafe fn recv_in_ptr(&self, slot: *mut T) -> impl Future<Output=RecvInResult> {
        WaitLocalRecv::new(unsafe { &mut *self.inner.get() }, slot)
    }

    generate_try_recv_in_ptr!();

    async fn receiver_close(&self) {
        let inner = unsafe { &mut *self.inner.get() };
        close(inner);
    }
}

impl<T> AsyncSender<T> for LocalChannel<T> {
    fn send(&self, value: T) -> impl Future<Output=SendResult<T>> {
        WaitLocalSend::new(value, unsafe { &mut *self.inner.get() })
    }

    generate_try_send!();

    async fn sender_close(&self) {
        let inner = unsafe { &mut *self.inner.get() };
        close(inner);
    }
}

unsafe impl<T> Sync for LocalChannel<T> {}

// endregion

/// ```fail_compile
/// use std::marker::PhantomData;
/// use orengine::sync::{AsyncChannel, AsyncSender, LocalChannel};
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// struct NonSend {
///     value: i32,
///     // impl !Send
///     no_send_marker: PhantomData<*const ()>,
/// }
///
/// async fn test() {
///     let channel = LocalChannel::bounded(1);
///
///     check_send(channel.send(NonSend { value: 1, no_send_marker: PhantomData })).await;
/// }
/// ```
///
/// ```fail_compile
/// use std::marker::PhantomData;
/// use orengine::sync::{AsyncChannel, AsyncReceiver, LocalChannel};
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// struct NonSend {
///     value: i32,
///     // impl !Send
///     no_send_marker: PhantomData<*const ()>,
/// }
///
/// async fn test() {
///     let channel = LocalChannel::<NonSend>::bounded(1);
///
///     check_send(channel.recv().await);
/// }
/// ```
///
/// ```fail_compile
/// use std::marker::PhantomData;
/// use orengine::sync::{AsyncChannel, AsyncSender, LocalChannel};
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// async fn test() {
///     let channel = LocalChannel::bounded(1);
///
///     check_send(channel.send(1)).await;
/// }
/// ```
///
/// ```fail_compile
/// use std::marker::PhantomData;
/// use orengine::sync::{AsyncChannel, AsyncReceiver, LocalChannel};
/// use orengine::yield_now;
///
/// fn check_send<T: Send>(value: T) -> T { value }
///
/// async fn test() {
///     let channel = LocalChannel::<usize>::bounded(1);
///
///     check_send(channel.recv().await);
/// }
/// ```
#[allow(dead_code, reason = "It is used only in compile tests")]
fn test_compile_local() {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::sync::{local_scope, RecvResult, TryRecvResult};
    use crate::utils::droppable_element::DroppableElement;
    use crate::utils::SpinLock;
    use crate::{yield_now, Local};
    use std::sync::Arc;

    #[orengine_macros::test_local]
    fn test_local_zero_capacity() {
        let ch = LocalChannel::bounded(0);
        let ch_ref = &ch;

        local_scope(|scope| async {
            scope.spawn(async move {
                ch_ref.send(1).await.unwrap();

                yield_now().await;

                ch_ref.send(2).await.unwrap();
                ch_ref.close().await;
            });

            let res = ch.recv().await.unwrap();
            assert_eq!(res, 1);
            let res = ch.recv().await.unwrap();
            assert_eq!(res, 2);

            match ch.send(2).await {
                SendResult::Closed(value) => assert_eq!(value, 2),
                _ => panic!("should be closed"),
            };
        })
            .await;
    }

    #[orengine_macros::test_local]
    fn test_local_unbounded() {
        let ch = LocalChannel::unbounded();
        let ch_ref = &ch;

        local_scope(|scope| async {
            scope.spawn(async move {
                ch_ref.send(1).await.unwrap();

                yield_now().await;

                for i in 2..100 {
                    ch_ref.send(i).await.unwrap();
                }

                ch_ref.close().await;
            });

            for i in 1..100 {
                let res = ch.recv().await.unwrap();
                assert_eq!(res, i);
            }

            assert!(matches!(ch.recv().await, RecvResult::Closed), "should be closed");
        })
            .await;
    }

    #[orengine_macros::test_local]
    fn test_local_channel_try() {
        let ch = LocalChannel::bounded(1);

        assert!(matches!(ch.try_recv(), TryRecvResult::Empty), "should be empty");
        assert!(matches!(ch.try_send(1), TrySendResult::Ok), "should be empty");
        assert!(matches!(ch.try_recv(), TryRecvResult::Ok(1)), "should be not empty");
        assert!(matches!(ch.try_send(2), TrySendResult::Ok), "should be empty");
        match ch.try_send(3) {
            TrySendResult::Ok => { panic!("should be full") }
            TrySendResult::Full(value) => { assert_eq!(value, 3) }
            TrySendResult::Locked(_) => { panic!("unreachable") }
            TrySendResult::Closed(_) => { panic!("should not be closed") }
        }

        ch.close().await;

        assert!(matches!(ch.try_recv(), TryRecvResult::Closed), "should be closed");
        match ch.try_send(4) {
            TrySendResult::Ok => { panic!("should be not empty") }
            TrySendResult::Full(_) => { panic!("should be not full") }
            TrySendResult::Locked(_) => { panic!("unreachable") }
            TrySendResult::Closed(value) => { assert_eq!(value, 4) }
        }
    }

    const N: usize = 125;

    // case 1 - send N and recv N. No wait
    // case 2 - send N and recv (N + 1). Wait for recv
    // case 3 - send (N + 1) and recv N. Wait for send
    // case 4 - send (N + 1) and recv (N + 1). Wait for send and wait for recv

    #[orengine_macros::test_local]
    fn test_local_channel_case1() {
        let ch = LocalChannel::bounded(N);
        let ch_ref = &ch;

        local_scope(|scope| async {
            scope.spawn(async move {
                for i in 0..N {
                    ch_ref.send(i).await.unwrap();
                }

                yield_now().await;

                ch_ref.close().await;
            });

            for i in 0..N {
                let res = ch.recv().await.unwrap();
                assert_eq!(res, i);
            }

            assert!(matches!(ch.recv().await, RecvResult::Closed), "should be closed");
        })
            .await;
    }

    #[orengine_macros::test_local]
    fn test_local_channel_case2() {
        let ch = LocalChannel::bounded(N);
        let ch_ref = &ch;

        local_scope(|scope| async {
            scope.spawn(async move {
                for i in 0..=N {
                    let res = ch_ref.recv().await.unwrap();
                    assert_eq!(res, i);
                }

                ch_ref.close().await;
            });

            for i in 0..N {
                ch.send(i).await.unwrap();
            }

            yield_now().await;

            ch.send(N).await.unwrap();
        })
            .await;
    }

    #[orengine_macros::test_local]
    fn test_local_channel_case3() {
        let ch = LocalChannel::bounded(N);
        let ch_ref = &ch;

        local_scope(|scope| async {
            scope.spawn(async move {
                for i in 0..N {
                    let res = ch_ref.recv().await.unwrap();
                    assert_eq!(res, i);
                }

                yield_now().await;

                let res = ch_ref.recv().await.unwrap();
                assert_eq!(res, N);
            });

            for i in 0..=N {
                ch.send(i).await.unwrap();
            }
        })
            .await;
    }

    #[orengine_macros::test_local]
    fn test_local_channel_case4() {
        let ch = LocalChannel::bounded(N);
        let ch_ref = &ch;

        local_scope(|scope| async {
            scope.spawn(async move {
                for i in 0..=N {
                    let res = ch_ref.recv().await.unwrap();
                    assert_eq!(res, i);
                }
            });

            for i in 0..=N {
                ch.send(i).await.unwrap();
            }
        })
            .await;
    }

    #[orengine_macros::test_local]
    fn test_local_channel_split() {
        let ch = LocalChannel::bounded(N);
        let (tx, rx) = ch.split();

        local_scope(|scope| async {
            scope.spawn(async {
                for i in 0..=N * 2 {
                    let res = rx.recv().await.unwrap();
                    assert_eq!(res, i);
                }
            });

            for i in 0..=N * 3 {
                tx.send(i).await.unwrap();
            }
        })
            .await;
    }

    #[orengine_macros::test_local]
    fn test_drop_local_channel() {
        let dropped = Arc::new(SpinLock::new(Vec::new()));
        let channel = LocalChannel::bounded(1);

        let _ = channel
            .send(DroppableElement::new(1, dropped.clone()))
            .await;
        let mut prev_elem = DroppableElement::new(2, dropped.clone());
        channel.recv_in(&mut prev_elem).await.unwrap();
        assert_eq!(prev_elem.value, 1);
        assert_eq!(dropped.lock().as_slice(), [2]);

        let _ = channel
            .send(DroppableElement::new(3, dropped.clone()))
            .await;
        unsafe { channel.recv_in_ptr(&mut prev_elem).await.unwrap() };
        assert_eq!(prev_elem.value, 3);
        assert_eq!(dropped.lock().as_slice(), [2]);

        channel.close().await;

        match channel.send(DroppableElement::new(5, dropped.clone())).await {
            SendResult::Closed(elem) => {
                assert_eq!(elem.value, 5);
                assert_eq!(dropped.lock().as_slice(), [2]);
            }
            _ => panic!("should be closed"),
        }
        assert_eq!(dropped.lock().as_slice(), [2, 5]);
    }

    #[orengine_macros::test_local]
    fn test_drop_local_channel_split() {
        let channel = LocalChannel::bounded(1);
        let dropped = Arc::new(SpinLock::new(Vec::new()));
        let (sender, receiver) = channel.split();

        let _ = sender.send(DroppableElement::new(1, dropped.clone())).await;
        let mut prev_elem = DroppableElement::new(2, dropped.clone());
        receiver.recv_in(&mut prev_elem).await.unwrap();
        assert_eq!(prev_elem.value, 1);
        assert_eq!(dropped.lock().as_slice(), [2]);

        let _ = sender.send(DroppableElement::new(3, dropped.clone())).await;
        unsafe { receiver.recv_in_ptr(&mut prev_elem).await.unwrap() };
        assert_eq!(prev_elem.value, 3);
        assert_eq!(dropped.lock().as_slice(), [2]);

        sender.sender_close().await;
        match channel.send(DroppableElement::new(5, dropped.clone())).await {
            SendResult::Closed(elem) => {
                assert_eq!(elem.value, 5);
                assert_eq!(dropped.lock().as_slice(), [2]);
            }
            _ => panic!("should be closed"),
        }
        assert_eq!(dropped.lock().as_slice(), [2, 5]);
    }

    async fn stress_test_local_channel_try(channel: LocalChannel<usize>) {
        const PAR: usize = 10;
        const COUNT: usize = 100;

        for _ in 0..10 {
            let res = Local::new(0);

            local_scope(|scope| async {
                for i in 0..PAR {
                    scope.spawn(async {
                        if i % 2 == 0 {
                            for j in 0..COUNT {
                                loop {
                                    match channel.try_send(j) {
                                        TrySendResult::Ok => break,
                                        TrySendResult::Full(_) => {
                                            yield_now().await;
                                        }
                                        TrySendResult::Closed(_) => panic!("send failed"),
                                        _ => {}
                                    }
                                }
                            }
                        } else {
                            for j in 0..COUNT {
                                channel.send(j).await.unwrap();
                            }
                        }
                    });

                    scope.spawn(async {
                        if i % 2 == 0 {
                            for _ in 0..COUNT {
                                loop {
                                    match channel.try_recv() {
                                        TryRecvResult::Ok(v) => {
                                            *res.get_mut() += v;
                                            break;
                                        }
                                        TryRecvResult::Empty => {
                                            yield_now().await;
                                        }
                                        TryRecvResult::Closed => panic!("recv failed"),
                                        _ => {}
                                    }
                                }
                            }
                        } else {
                            for _ in 0..COUNT {
                                let r = channel.recv().await.unwrap();
                                *res.get_mut() += r;
                            }
                        }
                    });
                }
            }).await;

            assert_eq!(*res, PAR * COUNT * (COUNT - 1) / 2);
        }
    }

    #[orengine_macros::test_local]
    fn stress_test_local_channel_try_unbounded() {
        stress_test_local_channel_try(LocalChannel::unbounded()).await;
    }

    #[orengine_macros::test_local]
    fn stress_test_local_channel_try_bounded() {
        stress_test_local_channel_try(LocalChannel::bounded(1024)).await;
    }
}