//! This module contains utilities for parallel testing via
//! [`sched_future_to_another_thread`] or [`sched_future`](ExecutorPool::sched_future).
use crate::bug_message::BUG_MESSAGE;
use crate::runtime::Config;
use crate::sync::{AsyncChannel, AsyncReceiver, AsyncSender, Channel, RecvResult, SendResult};
use crate::{local_executor, Executor};
use crossbeam::queue::SegQueue;
use std::future::Future;
use std::panic::UnwindSafe;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use std::{panic, ptr, thread};

struct Result {
    future_result: thread::Result<()>,
    sender: Arc<Channel<Job>>,
}

unsafe impl Send for Result {}

/// Short type alias for `Arc<Channel<(thread::Result<()>, Arc<Channel<Job>>)>>`
type ResultSender = Arc<Channel<Result>>;

/// `Job` is a wrapper for a [`Future`] via polling it and sending the result
/// (caught panic) to the `result_sender`.
///
/// It also contains a `sender` to acquired [`Executor`] that will be released after
/// the task is done.
struct Job {
    future: Box<dyn Future<Output=()> + UnwindSafe>,
    sender: Option<Arc<Channel<Job>>>,
    result_sender: ResultSender,
}

impl Job {
    /// Creates a new `Job` instance. Read [`Job`] for more information.
    pub(crate) fn new<Fut: Future<Output=()> + UnwindSafe + 'static>(
        future: Fut,
        channel: Arc<Channel<Self>>,
        result_channel: ResultSender,
    ) -> Self {
        Self {
            future: Box::new(future),
            sender: Some(channel),
            result_sender: result_channel,
        }
    }
}

impl Future for Job {
    type Output = ();

    fn poll(self: Pin<&mut Self>, mut cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        let mut unwind_safe_cx = panic::AssertUnwindSafe(&mut cx);
        let mut unwind_safe_future = {
            panic::AssertUnwindSafe(ptr::from_mut(this.future.as_mut()))
        };
        let handle = panic::catch_unwind(move || {
            let pinned_future = unsafe {
                Pin::new_unchecked(&mut **unwind_safe_future)
            };
            pinned_future.poll(*unwind_safe_cx)
        });

        if let Ok(poll_res) = handle {
            if poll_res.is_ready() {
                let sender = this.sender.take().unwrap();
                local_executor().exec_shared_future(async move {
                    let send_res = this
                        .result_sender
                        .send(Result {
                            future_result: Ok(()),
                            sender,
                        })
                        .await;
                    assert!(matches!(send_res, SendResult::Ok), "{BUG_MESSAGE}");
                });

                return Poll::Ready(());
            }

            Poll::Pending
        } else {
            let sender = this.sender.take().unwrap();
            local_executor().exec_shared_future(async move {
                let send_res = this
                    .result_sender
                    .send(Result {
                        future_result: Err(Box::new(handle.unwrap_err())),
                        sender,
                    })
                    .await;
                assert!(matches!(send_res, SendResult::Ok), "{BUG_MESSAGE}");
            });

            Poll::Ready(())
        }
    }
}

#[allow(clippy::non_send_fields_in_send_ty)] // we care about Send manually
unsafe impl Send for Job {}

/// `ExecutorPoolJoinHandle` is used to wait for the task sent to the [`ExecutorPool`]
/// to complete. It can be gotten by [`ExecutorPool::sched_future()`].
///
/// If you don't need to wait,
/// use [`sched_future_to_another_thread`].
///
/// # Panic
///
/// If not [`joined`](ExecutorPoolJoinHandle::join) before drop.
pub struct ExecutorPoolJoinHandle {
    was_joined: bool,
    channel: ResultSender,
    pool: &'static ExecutorPool,
}

impl ExecutorPoolJoinHandle {
    /// Creates a new `ExecutorPoolJoinHandle` instance.
    fn new(
        channel: ResultSender,
        pool: &'static ExecutorPool,
    ) -> Self {
        Self {
            was_joined: false,
            channel,
            pool,
        }
    }

    /// Waits for the task sent to the [`ExecutorPool`] to complete.
    ///
    /// # Panics
    ///
    /// If test fn was panicked. It is used to `should_panic`.
    pub async fn join(mut self) {
        self.was_joined = true;
        let res = self.channel.recv().await.unwrap();
        self.pool.senders_to_executors.push(res.sender);

        if let Err(err) = res.future_result {
            panic::resume_unwind(err);
        }
    }
}

unsafe impl Send for ExecutorPoolJoinHandle {}

impl Drop for ExecutorPoolJoinHandle {
    fn drop(&mut self) {
        assert!(
            self.was_joined,
            "ExecutorPoolJoinHandle::join() must be called! \
        If you don't want to wait result immediately, put it somewhere and join it later."
        );
    }
}

/// `ExecutorPool` allows to reuse [`Executor`] instances. It is used in tests.
///
/// # Thread safety
///
/// `ExecutorPool` is thread-safe.
pub struct ExecutorPool {
    senders_to_executors: SegQueue<Arc<Channel<Job>>>,
}

unsafe impl Send for ExecutorPool {}

/// Returns [`Config`] for creating [`Executor`] in the [`pool`](ExecutorPool).
fn executor_pool_cfg() -> Config {
    Config::default().disable_work_sharing()
}

/// `EXECUTOR_POOL` allows to reuse [`Executor`] instances. It is used to test parallelism.
static EXECUTOR_POOL: ExecutorPool = ExecutorPool::new();

impl ExecutorPool {
    /// Creates a new `ExecutorPool` instance.
    pub(crate) const fn new() -> Self {
        Self {
            senders_to_executors: SegQueue::new(),
        }
    }

    /// Creates a new executor in new thread and returns its [`channel`](Channel).
    fn new_executor() -> Arc<Channel<Job>> {
        let channel = Arc::new(Channel::bounded(0));
        let channel_clone = channel.clone();
        thread::spawn(move || {
            let ex = Executor::init_with_config(executor_pool_cfg());
            ex.run_and_block_on_shared(async move {
                while let RecvResult::Ok(job) = channel_clone.recv().await {
                    job.await;
                }
            })
                .expect(BUG_MESSAGE);
        });

        channel
    }

    /// Schedules a future to any free executor in the [`pool`](ExecutorPool).
    ///
    /// It is used to test parallelism.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::sync::Arc;
    /// use std::sync::atomic::AtomicUsize;
    /// use std::sync::atomic::Ordering::SeqCst;
    /// use orengine::test::{run_test_and_block_on_shared, ExecutorPool};
    /// use orengine::yield_now;
    ///
    /// async fn awesome_function(atomic_to_sync_test: Arc<AtomicUsize>) {
    ///     atomic_to_sync_test.fetch_add(1, SeqCst);
    ///     yield_now().await;
    ///     atomic_to_sync_test.fetch_add(1, SeqCst);
    /// }
    ///
    /// #[cfg(test)]
    /// fn test_awesome_function() {
    ///     run_test_and_block_on_shared(async {
    ///         let atomic_to_sync_test = Arc::new(AtomicUsize::new(0));
    ///         let mut handles = Vec::with_capacity(10);
    ///
    ///         for _ in 0..10 {
    ///             let join = ExecutorPool::sched_future(
    ///                 awesome_function(atomic_to_sync_test.clone())
    ///             ).await;
    ///
    ///             handles.push(join);
    ///         }
    ///
    ///         for handle in handles {
    ///             handle.join().await;
    ///         }
    ///
    ///         assert_eq!(atomic_to_sync_test.load(SeqCst), 20);
    ///     });
    /// }
    /// ```
    #[allow(clippy::missing_panics_doc)] // It panics only when a bug is occurred.
    pub async fn sched_future<Fut>(future: Fut) -> ExecutorPoolJoinHandle
    where
        Fut: Future<Output=()> + Send + 'static + UnwindSafe,
    {
        let result_channel = Arc::new(Channel::bounded(0));
        let sender = EXECUTOR_POOL
            .senders_to_executors
            .pop()
            .unwrap_or_else(Self::new_executor);

        let send_res = sender
            .send(Job::new(future, sender.clone(), result_channel.clone()))
            .await;
        assert!(matches!(send_res, SendResult::Ok), "{BUG_MESSAGE}");

        ExecutorPoolJoinHandle::new(result_channel, &EXECUTOR_POOL)
    }
}

/// Schedules a future to any free executor in the [`pool`](ExecutorPool).
///
/// It is used to test parallelism.
///
/// # The difference with [`ExecutorPool::sched_future`]
///
/// [`sched_future_to_another_thread`] is a wrapper for [`ExecutorPool::sched_future`]
/// that joins the [`ExecutorPoolJoinHandle`] in new task.
///
/// # Panic or unexpected behavior
///
/// If called in thread where [`Executor`] is not initialized.
///
/// # Example
///
/// ```rust
/// use std::sync::Arc;
/// use std::sync::atomic::AtomicUsize;
/// use std::sync::atomic::Ordering::SeqCst;
/// use orengine::sync::WaitGroup;
/// use orengine::test::{run_test_and_block_on_shared, sched_future_to_another_thread};
/// use orengine::yield_now;
///
/// async fn awesome_function(atomic_to_sync_test: Arc<AtomicUsize>, wg: Arc<WaitGroup>) {
///     atomic_to_sync_test.fetch_add(1, SeqCst);
///     yield_now().await;
///     atomic_to_sync_test.fetch_add(1, SeqCst);
///     wg.done();
/// }
///
/// #[cfg(test)]
/// fn test_awesome_function() {
///     run_test_and_block_on_shared(async {
///         let atomic_to_sync_test = Arc::new(AtomicUsize::new(0));
///         let wg = Arc::new(WaitGroup::new());
///
///         for _ in 0..10 {
///             wg.inc();
///             sched_future_to_another_thread(awesome_function(atomic_to_sync_test.clone(), wg.clone()));
///         }
///
///         wg.wait().await;
///         assert_eq!(atomic_to_sync_test.load(SeqCst), 20);
///     });
/// }
/// ```
pub fn sched_future_to_another_thread<Fut>(future: Fut)
where
    Fut: Future<Output=()> + Send + 'static + UnwindSafe,
{
    local_executor().exec_shared_future(async move {
        let handle = ExecutorPool::sched_future(future).await;

        handle.join().await;
    });
}
