use std::future::Future;
use std::mem::MaybeUninit;
use std::ptr::drop_in_place;

/// The result of an asynchronous send operation.
///
/// Represents the possible outcomes of sending a value into a channel.
///
/// # Variants
///
/// - [`Ok`](SendResult::Ok): The value was successfully sent into the channel.
///
/// - [`Closed`](SendResult::Closed): The channel was closed before the value could be sent.
#[must_use]
pub enum SendResult<T> {
    /// The value was successfully sent into the channel.
    Ok,
    /// The channel was closed before the value could be sent.
    Closed(T),
}

impl<T> SendResult<T> {
    /// # Panics
    ///
    /// If the channel is [`closed`](SendResult::Closed).
    pub fn unwrap(self) {
        if matches!(self, Self::Closed(_)) {
            panic!("Unwrap on SendResult::Err: channel is closed");
        }
    }
}

/// The result of a non-blocking send attempt.
///
/// Used for scenarios where the sender attempts to send a value
/// without waiting for the channel to become available.
///
/// # Variants
///
/// - [`Ok`](TrySendResult::Ok): The value was successfully sent.
///
/// - [`Full`](TrySendResult::Full): The channel is full. Contains the value that could not be sent.
///
/// - [`Locked`](TrySendResult::Locked): The channel is locked, indicating temporary unavailability.
///   Contains the value that could not be sent.
///
/// - [`Closed`](TrySendResult::Closed): The channel is closed.
///   Contains the value that could not be sent.
#[must_use]
pub enum TrySendResult<T> {
    /// The value was successfully sent.
    Ok,
    /// The channel is full. Contains the value that could not be sent.
    Full(T),
    /// The channel is locked, indicating temporary unavailability.
    /// Contains the value that could not be sent.
    Locked(T),
    /// The channel is closed. Contains the value that could not be sent.
    Closed(T),
}

impl<T> TrySendResult<T> {
    /// # Panics
    ///
    /// If the channel is [`closed`](TrySendResult::Closed), [`full`](TrySendResult::Full),
    /// or [`locked`](TrySendResult::Locked).
    pub fn unwrap(self) {
        match self {
            Self::Ok => (),
            Self::Full(_) => panic!("Unwrap on TrySendResult::Full."),
            Self::Locked(_) => panic!("Unwrap on TrySendResult::Locked."),
            Self::Closed(_) => panic!("Unwrap on TrySendResult::Closed."),
        }
    }
}

/// The result of an asynchronous `receive` operation.
///
/// Indicates the result of attempting to receive a value from a channel.
///
/// # Variants
///
/// - [`Ok`](RecvInResult::Ok): The value was successfully received.
///
/// - [`Closed`](RecvInResult::Closed): The channel is closed, and no more values can be received.
#[must_use]
pub enum RecvInResult {
    /// The value was successfully received.
    Ok,
    /// The channel is closed, and no more values can be received.
    Closed,
}

impl RecvInResult {
    /// # Panics
    ///
    /// If the channel is [`closed`](RecvInResult::Closed).
    pub fn unwrap(self) {
        if matches!(self, Self::Closed) {
            panic!("Unwrap on RecvInResult::Closed.");
        }
    }
}

/// The result of a non-blocking receive attempt.
///
/// Used for scenarios where the receiver attempts to receive a value
/// without waiting for one to be available in the channel.
///
/// # Variants
///
/// - [`Ok`](TryRecvInResult::Ok): The value was successfully received.
///
/// - [`Empty`](TryRecvInResult::Empty): The channel is empty; no values are currently available.
///
/// - [`Locked`](TryRecvInResult::Locked): The channel is locked, indicating temporary unavailability.
///
/// - [`Closed`](TryRecvInResult::Closed): The channel is closed, and no more values can be received.
#[must_use]
pub enum TryRecvInResult {
    /// The value was successfully received.
    Ok,
    /// The channel is empty; no values are currently available.
    Empty,
    /// The channel is locked, indicating temporary unavailability.
    Locked,
    /// The channel is closed, and no more values can be received.
    Closed,
}

impl TryRecvInResult {
    /// # Panics
    ///
    /// If the channel is [`closed`](TryRecvInResult::Closed), [`empty`](TryRecvInResult::Empty),
    /// or [`locked`](TryRecvInResult::Locked).
    pub fn unwrap(self) {
        match self {
            Self::Ok => (),
            Self::Empty => panic!("Unwrap on TryRecvInResult::Empty."),
            Self::Locked => panic!("Unwrap on TryRecvInResult::Locked."),
            Self::Closed => panic!("Unwrap on TryRecvInResult::Closed."),
        }
    }
}

/// The result of an asynchronous `receive` operation.
///
/// Represents a successful reception of a value from the channel or
/// a notification that the channel is closed.
///
/// # Variants
///
/// - [`Ok`](RecvResult::Ok): A value was successfully received from the channel.
///
/// - [`Closed`](RecvResult::Closed): The channel is closed, and no more values can be received.
#[must_use]
pub enum RecvResult<T> {
    /// A value was successfully received from the channel.
    Ok(T),
    /// The channel is closed, and no more values can be received.
    Closed,
}

impl<T> RecvResult<T> {
    /// # Panics
    ///
    /// If the channel is [`closed`](RecvResult::Closed).
    pub fn unwrap(self) -> T {
        match self {
            Self::Ok(v) => v,
            Self::Closed => panic!("Unwrap on RecvResult::Closed."),
        }
    }
}

/// The result of a non-blocking receive attempt.
///
/// Provides an outcome for an attempt to receive a value from the channel without
/// waiting for one to become available.
///
/// # Variants
///
/// - [`Ok`](TryRecvResult::Ok): A value was successfully received from the channel.
///
/// - [`Empty`](TryRecvResult::Empty): The channel is empty; no values are currently available.
///
/// - [`Locked`](TryRecvResult::Locked): The channel is locked, indicating temporary unavailability.
///
/// - [`Closed`](TryRecvResult::Closed): The channel is closed, and no more values can be received.
#[must_use]
pub enum TryRecvResult<T> {
    /// A value was successfully received from the channel.
    Ok(T),
    /// The channel is empty; no values are currently available.
    Empty,
    /// The channel is locked, indicating temporary unavailability.
    Locked,
    /// The channel is closed, and no more values can be received.
    Closed,
}

impl<T> TryRecvResult<T> {
    /// # Panics
    ///
    /// If the channel is [`closed`](TryRecvResult::Closed), [`empty`](TryRecvResult::Empty),
    /// or [`locked`](TryRecvResult::Locked).
    pub fn unwrap(self) -> T {
        match self {
            Self::Ok(v) => v,
            Self::Empty => panic!("Unwrap on TryRecvResult::Empty."),
            Self::Locked => panic!("Unwrap on TryRecvResult::Locked."),
            Self::Closed => panic!("Unwrap on TryRecvResult::Closed."),
        }
    }
}

/// The `AsyncSender` allows sending values into the [`channel`](AsyncChannel).
///
/// It provides blocking [`send`](AsyncSender::send) and non-blocking
/// [`try_send`](AsyncSender::try_send) methods.
///
/// # Example
///
/// ```rust
/// use orengine::sync::{AsyncChannel, AsyncReceiver, AsyncSender};
///
///  async fn foo() {
///     let channel = orengine::sync::Channel::bounded(2); // capacity = 2
///     let (sender, receiver) = channel.split();
///
///     sender.send(1).await.unwrap();
///     let res = receiver.recv().await.unwrap();
///     assert_eq!(res, 1);
/// }
/// ```
pub trait AsyncSender<T> {
    /// Sends a value into the [`channel`](AsyncChannel).
    ///
    /// Wait until the [`channel`](AsyncChannel) is available or
    /// the [`channel`](AsyncChannel) is closed.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::time::Duration;
    /// use orengine::sleep;
    /// use orengine::sync::{shared_scope, AsyncChannel, AsyncReceiver, AsyncSender};
    ///
    ///  async fn foo() {
    ///     let channel = orengine::sync::Channel::bounded(1);
    ///     let (sender, receiver) = channel.split();
    ///     let start = std::time::Instant::now();
    ///
    ///     shared_scope(|scope| async {
    ///         scope.spawn(async move {
    ///             sleep(Duration::from_millis(100)).await;
    ///             receiver.recv().await.unwrap();
    ///         });
    ///
    ///         sender.send(1).await.unwrap();
    ///         assert!(start.elapsed() < Duration::from_millis(100));
    ///         sender.send(2).await.unwrap(); // blocks, because the channel is full
    ///         assert!(start.elapsed() >= Duration::from_millis(100));
    ///     }).await;
    /// }
    /// ```
    fn send(&self, value: T) -> impl Future<Output=SendResult<T>>;

    /// Tries to send a value into the [`channel`](AsyncChannel).
    ///
    /// If the [`channel`](AsyncChannel) is full, returns [`TrySendResult::Full`].
    ///
    /// If the [`channel`](AsyncChannel) is locked, returns [`TrySendResult::Locked`].
    ///
    /// If the [`channel`](AsyncChannel) is closed, returns [`TrySendResult::Closed`].
    ///
    /// Else, the value is immediately sent.
    ///
    /// You can find an example in [`AsyncSender::try_send`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::sync::{AsyncChannel, AsyncSender, TrySendResult};
    ///
    /// async fn foo() {
    ///     let channel = orengine::sync::Channel::bounded(1);
    ///     let (sender, receiver) = channel.split();
    ///
    ///     assert!(matches!(sender.try_send(1), TrySendResult::Ok));
    ///     assert!(matches!(sender.try_send(2), TrySendResult::Full(_)));
    ///     channel.close().await;
    ///     assert!(matches!(sender.try_send(3), TrySendResult::Closed(_)));
    /// }
    /// ```
    fn try_send(&self, value: T) -> TrySendResult<T>;

    /// Closes the [`channel`](AsyncChannel) associated with this sender.
    fn sender_close(&self) -> impl Future<Output=()>;
}

/// The `AsyncReceiver` allows receiving values from the [`channel`](AsyncChannel).
///
/// It provides blocking [`recv`](AsyncReceiver::recv) and non-blocking
/// [`try_recv`](AsyncReceiver::try_recv) methods.
///
/// # Example
///
/// ```rust
/// use orengine::sync::{AsyncChannel, AsyncReceiver, AsyncSender};
///
/// async fn foo() {
///     let channel = orengine::sync::Channel::bounded(2); // capacity = 2
///     let (sender, receiver) = channel.split();
///
///     sender.send(1).await.unwrap();
///     let res = receiver.recv().await.unwrap();
///     assert_eq!(res, 1);
/// }
/// ```
pub trait AsyncReceiver<T> {
    /// Asynchronously receives a value from the [`channel`](AsyncChannel) to the provided `slot`.
    ///
    /// If the [`channel`](AsyncChannel) is empty, the receiver waits until a value
    /// is available or the [`channel`](AsyncChannel) is closed.
    ///
    /// Else, the value is immediately received.
    ///
    /// # On close
    ///
    /// Returns [`RecvInResult::Closed`] if the [`channel`](AsyncChannel) is closed.
    ///
    /// # Attention
    ///
    /// __Doesn't drop__ the previous value in the `slot`.
    ///
    /// # Safety
    ///
    /// - Provided pointer is valid and aligned;
    ///
    /// - Previous value is dropped.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::ptr::drop_in_place;
    /// use orengine::sync::{AsyncReceiver, RecvInResult};
    ///
    /// # type Payload = i32;
    ///
    /// // Must be dropped.
    /// struct Msg { value: Box<Payload> }
    ///
    /// # fn process_msg(msg: &Msg) {}
    ///
    /// async fn handle_messages<R: AsyncReceiver<Msg>>(receiver: R) {
    ///     let mut msg = unsafe { std::mem::MaybeUninit::uninit() };
    ///
    ///     loop {
    ///         // SAFETY: previous value is dropped or absent
    ///         match unsafe { receiver.recv_in_ptr(msg.as_mut_ptr()) }.await {
    ///             RecvInResult::Ok => {
    ///                 process_msg(unsafe { msg.assume_init_ref() });
    ///                 // SAFETY: value exists
    ///                 unsafe { drop_in_place(msg.as_mut_ptr()) };
    ///             }
    ///             RecvInResult::Closed => return
    ///         }
    ///     }
    /// }
    /// ```
    unsafe fn recv_in_ptr(&self, slot: *mut T) -> impl Future<Output=RecvInResult>;

    /// Tries to receive a value from the [`channel`](AsyncChannel) to the provided `slot`.
    ///
    /// If the [`channel`](AsyncChannel) is empty, the receiver returns [`TryRecvInResult::Empty`].
    ///
    /// If the [`channel`](AsyncChannel) is locked, the receiver returns [`TryRecvInResult::Locked`].
    ///
    /// If the [`channel`](AsyncChannel) is closed, the receiver returns [`TryRecvInResult::Closed`].
    ///
    /// Else, the value is immediately received.
    ///
    /// # The difference between `try_recv_in_ptr` and `recv_in_ptr`
    ///
    /// `try_recv_in_ptr` doesn't block current task.
    ///
    /// # Attention
    ///
    /// __Doesn't drop__ the previous value in the `slot`.
    ///
    /// # Safety
    ///
    /// - Provided pointer is valid and aligned;
    ///
    /// - Previous value is dropped.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::ptr::drop_in_place;
    /// use orengine::sync::{AsyncReceiver, TryRecvInResult};
    ///
    /// # type Payload = i32;
    ///
    /// // Must be dropped.
    /// struct Msg { value: Box<Payload> }
    ///
    /// # fn process_msg(msg: &Msg) {}
    ///
    /// async fn handle_new_messages<R: AsyncReceiver<Msg>>(receiver: R) -> Result<usize, ()> {
    ///     let mut msg = unsafe { std::mem::MaybeUninit::uninit() };
    ///     let mut processed = 0;
    ///
    ///     loop {
    ///         // SAFETY: previous value is dropped or absent
    ///         match unsafe { receiver.try_recv_in_ptr(msg.as_mut_ptr()) } {
    ///             TryRecvInResult::Ok => {
    ///                 process_msg(unsafe { msg.assume_init_ref() });
    ///                 // SAFETY: value exists
    ///                 unsafe { drop_in_place(msg.as_mut_ptr()) };
    ///                 processed += 1;
    ///             }
    ///             TryRecvInResult::Empty | TryRecvInResult::Locked => {
    ///                 return Ok(processed);
    ///             },
    ///             TryRecvInResult::Closed => return Err(())
    ///         }
    ///     }
    /// }
    /// ```
    unsafe fn try_recv_in_ptr(&self, slot: *mut T) -> TryRecvInResult;

    /// Asynchronously receives a value from the [`channel`](AsyncChannel) to the provided `slot`.
    ///
    /// If the [`channel`](AsyncChannel) is empty, the receiver waits until a value
    /// is available or the [`channel`](AsyncChannel) is closed.
    ///
    /// Else, the value is immediately received.
    ///
    /// # On close
    ///
    /// Returns [`RecvInResult::Closed`] if the [`channel`](AsyncChannel) is closed.
    ///
    /// # Attention
    ///
    /// __Drops__ the previous value in the `slot`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::sync::{AsyncReceiver, RecvInResult, RecvResult};
    ///
    /// # type Payload = i32;
    ///
    /// // Must be dropped.
    /// struct Msg { value: Box<Payload> }
    ///
    /// # fn process_msg(msg: &Msg) {}
    ///
    /// async fn handle_messages<R: AsyncReceiver<Msg>>(receiver: R) {
    ///     let mut msg = match receiver.recv().await {
    ///         RecvResult::Ok(msg) => {
    ///             process_msg(&msg);
    ///             msg
    ///         },
    ///         RecvResult::Closed => return
    ///     };
    ///
    ///     loop {
    ///         match receiver.recv_in(&mut msg).await {
    ///             RecvInResult::Ok => {
    ///                 process_msg(&msg);
    ///             }
    ///             RecvInResult::Closed => return
    ///         }
    ///     }
    /// }
    /// ```
    #[inline]
    fn recv_in(&self, slot: &mut T) -> impl Future<Output=RecvInResult> {
        unsafe {
            drop_in_place(slot);

            self.recv_in_ptr(slot)
        }
    }

    /// Tries to receive a value from the [`channel`](AsyncChannel) to the provided `slot`.
    ///
    /// If the [`channel`](AsyncChannel) is empty, returns [`TryRecvInResult::Empty`].
    ///
    /// If the [`channel`](AsyncChannel) is locked, returns [`TryRecvInResult::Locked`].
    ///
    /// If the [`channel`](AsyncChannel) is closed, returns [`TryRecvInResult::Closed`].
    ///
    /// Else, the value is immediately received.
    ///
    /// # The difference between `try_recv_in_ptr` and `recv_in_ptr`
    ///
    /// `try_recv_in_ptr` doesn't block current task.
    ///
    /// # Attention
    ///
    /// __Drops__ the previous value in the `slot`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::ptr::drop_in_place;
    /// use orengine::sync::{AsyncReceiver, TryRecvInResult, TryRecvResult};
    ///
    /// # type Payload = i32;
    ///
    /// // Must be dropped.
    /// struct Msg { value: Box<Payload> }
    ///
    /// # fn process_msg(msg: &Msg) {}
    ///
    /// async fn handle_new_messages<R: AsyncReceiver<Msg>>(receiver: R) -> Result<usize, ()> {
    ///     let mut msg = match receiver.try_recv() {
    ///         TryRecvResult::Ok(msg) => {
    ///             process_msg(&msg);
    ///             msg
    ///         },
    ///         TryRecvResult::Empty | TryRecvResult::Locked => return Ok(0),
    ///         TryRecvResult::Closed => return Err(())
    ///     };
    ///     let mut processed = 1;
    ///
    ///     loop {
    ///         match receiver.try_recv_in(&mut msg) {
    ///             TryRecvInResult::Ok => {
    ///                 process_msg(&msg);
    ///                 processed += 1;
    ///             }
    ///             TryRecvInResult::Empty | TryRecvInResult::Locked => {
    ///                 return Ok(processed);
    ///             },
    ///             TryRecvInResult::Closed => return Err(())
    ///         }
    ///     }
    /// }
    /// ```
    #[inline]
    fn try_recv_in(&self, slot: &mut T) -> TryRecvInResult {
        unsafe {
            drop_in_place(slot);

            self.try_recv_in_ptr(slot)
        }
    }

    /// Asynchronously receives a value from the [`channel`](AsyncChannel).
    ///
    /// If the [`channel`](AsyncChannel) is empty, returns [`TryRecvInResult::Empty`].
    ///
    /// If the [`channel`](AsyncChannel) is locked, returns [`TryRecvInResult::Locked`].
    ///
    /// If the [`channel`](AsyncChannel) is closed, returns [`TryRecvInResult::Closed`].
    ///
    /// Else, the value is immediately received.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::sync::{AsyncReceiver, TryRecvResult};
    ///
    /// # type Payload = i32;
    ///
    /// // Must be dropped.
    /// struct Msg { value: Box<Payload> }
    ///
    /// # fn process_msg(msg: &Msg) {}
    ///
    /// async fn handle_messages<R: AsyncReceiver<Msg>>(receiver: R) -> Result<usize, ()> {
    ///     let mut processed = 0;
    ///
    ///     loop {
    ///         match receiver.try_recv() {
    ///             TryRecvResult::Ok(msg) => {
    ///                 process_msg(&msg);
    ///                 processed += 1;
    ///             },
    ///             TryRecvResult::Empty | TryRecvResult::Locked => {
    ///                 return Ok(processed);
    ///             },
    ///             TryRecvResult::Closed => return Err(())
    ///         }
    ///     }
    /// }
    /// ```
    #[inline]
    fn recv(&self) -> impl Future<Output=RecvResult<T>> {
        async {
            let mut slot = MaybeUninit::uninit();
            unsafe {
                match self.recv_in_ptr(slot.as_mut_ptr()).await {
                    RecvInResult::Ok => RecvResult::Ok(slot.assume_init()),
                    RecvInResult::Closed => RecvResult::Closed,
                }
            }
        }
    }

    /// Tries to receive a value from the [`channel`](AsyncChannel).
    ///
    /// If the [`channel`](AsyncChannel) is empty, returns [`TryRecvInResult::Empty`].
    ///
    /// If the [`channel`](AsyncChannel) is locked, returns [`TryRecvInResult::Locked`].
    ///
    /// If the [`channel`](AsyncChannel) is closed, returns [`TryRecvInResult::Closed`].
    ///
    /// Else, the value is immediately received.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::sync::{AsyncReceiver, RecvResult};
    ///
    /// # type Payload = i32;
    ///
    /// // Must be dropped.
    /// struct Msg { value: Box<Payload> }
    ///
    /// # fn process_msg(msg: &Msg) {}
    ///
    /// async fn handle_messages<R: AsyncReceiver<Msg>>(receiver: R) {
    ///     loop {
    ///         match receiver.recv().await {
    ///             RecvResult::Ok(msg) => {
    ///                 process_msg(&msg);
    ///             }
    ///             RecvResult::Closed => return
    ///         }
    ///     }
    /// }
    /// ```
    #[inline]
    fn try_recv(&self) -> TryRecvResult<T> {
        let mut slot = MaybeUninit::uninit();
        unsafe {
            match self.try_recv_in_ptr(slot.as_mut_ptr()) {
                TryRecvInResult::Ok => TryRecvResult::Ok(slot.assume_init()),
                TryRecvInResult::Empty => TryRecvResult::Empty,
                TryRecvInResult::Locked => TryRecvResult::Locked,
                TryRecvInResult::Closed => TryRecvResult::Closed,
            }
        }
    }

    /// Closes the [`channel`](AsyncChannel) associated with this receiver.
    fn receiver_close(&self) -> impl Future<Output=()>;
}

/// The `Channel` provides an asynchronous communication channel between tasks.
///
/// It supports both [`bounded`](AsyncChannel::bounded) and [`unbounded`](AsyncChannel::unbounded)
/// channels for sending and receiving values.
///
/// If communication occurs between `local` tasks (read about `local` tasks in
/// [`Executor`](crate::Executor)), use [`LocalChannel`](crate::sync::LocalChannel).
///
/// Else use [`Channel`](crate::sync::Channel).
///
/// # Examples
///
/// ## Don't split
///
/// ```rust
/// use orengine::sync::{AsyncChannel, AsyncReceiver, AsyncSender};
///
/// async fn foo() {
///     let channel = orengine::sync::Channel::bounded(1); // capacity = 1
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
///     let channel = orengine::sync::Channel::bounded(1); // capacity = 1
///     let (sender, receiver) = channel.split();
///
///     sender.send(1).await.unwrap();
///     let res = receiver.recv().await.unwrap();
///     assert_eq!(res, 1);
/// }
/// ```
pub trait AsyncChannel<T>: AsyncSender<T> + AsyncReceiver<T> {
    /// The `AsyncSender` allows sending values into the [`channel`](AsyncChannel).
    ///
    /// It provides blocking [`send`](AsyncSender::send) and non-blocking
    /// [`try_send`](AsyncSender::try_send) methods.
    type Sender<'channel>: AsyncSender<T> + 'channel
    where
        Self: 'channel;

    /// The `AsyncReceiver` allows receiving values from the [`channel`](AsyncChannel).
    ///
    /// It provides blocking [`recv`](AsyncReceiver::recv) and non-blocking
    /// [`try_recv`](AsyncReceiver::try_recv) methods.
    type Receiver<'channel>: AsyncReceiver<T> + 'channel
    where
        Self: 'channel;

    /// Creates a bounded [`channel`](AsyncChannel) with a given capacity.
    ///
    /// A bounded channel limits the number of items that can be stored before sending blocks.
    /// Once the [`channel`](AsyncChannel) reaches its capacity,
    /// senders will block until space becomes available.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::sync::{AsyncChannel, AsyncSender};
    ///
    ///  async fn foo() {
    ///     let channel = orengine::sync::Channel::bounded(1);
    ///
    ///     channel.send(1).await.unwrap(); // not blocked
    ///     channel.send(2).await.unwrap(); // blocked because the channel is full
    /// }
    /// ```
    fn bounded(capacity: usize) -> Self;

    /// Creates an unbounded [`channel`](AsyncChannel).
    ///
    /// An unbounded channel allows senders to send an unlimited number of values.
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::sync::{AsyncChannel, AsyncSender};
    ///
    ///  async fn foo() {
    ///     let channel = orengine::sync::Channel::unbounded();
    ///
    ///     channel.send(1).await.unwrap(); // not blocked
    ///     channel.send(2).await.unwrap(); // not blocked
    /// }
    /// ```
    fn unbounded() -> Self;

    /// Returns [`AsyncSender`] and [`AsyncReceiver`] for the [`channel`](AsyncChannel).
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::sync::{local_scope, AsyncChannel, AsyncReceiver, AsyncSender};
    ///
    /// struct Actor1<Req: AsyncReceiver<usize>, Res: AsyncSender<usize>> {
    ///     req_ch: Req,
    ///     res_ch: Res
    /// }
    ///
    /// # async fn run_actor1<Req: AsyncReceiver<usize>, Res: AsyncSender<usize>>(actor: Actor1<Req, Res>) {}
    ///
    /// struct Actor2<Req: AsyncReceiver<usize>, Res: AsyncSender<usize>> {
    ///     req_ch: Req,
    ///     res_ch: Res
    /// }
    ///
    /// # async fn run_actor2<Req: AsyncReceiver<usize>, Res: AsyncSender<usize>>(actor: Actor2<Req, Res>) {}
    ///
    /// async fn start_actors<Ch1: AsyncChannel<usize>, Ch2: AsyncChannel<usize>>(ch1: Ch1, ch2: Ch2) {
    ///     let (actor1_res, actor1_req) = ch1.split();
    ///     let (actor2_res, actor2_req) = ch2.split();
    ///
    ///     let actor1 = Actor1 { req_ch: actor1_req, res_ch: actor2_res };
    ///     let actor2 = Actor2 { req_ch: actor2_req, res_ch: actor1_res };
    ///
    ///     local_scope(|scope| async {
    ///         scope.spawn(run_actor1(actor1));
    ///
    ///         run_actor2(actor2).await;
    ///     }).await;
    /// }
    /// ```
    fn split(&self) -> (Self::Sender<'_>, Self::Receiver<'_>);

    /// Closes the [`channel`](AsyncChannel).
    fn close(&self) -> impl Future<Output=()>;
}
