use crate::bug_message::BUG_MESSAGE;
use crate::runtime::{Config, Locality, Task};
use crate::sync::Channel;
use crate::{local_executor, Executor};
use crossbeam::queue::SegQueue;
use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex as STDMutex};
use std::task::Poll;
use std::{panic, thread};

/// `Job` is a wrapper for a task that implements the [`Future`] trait via polling
/// the associated [`Task`] and sending the result (caught panic) to the `result_sender`.
///
/// It also contains a `sender` to acquired [`Executor`] that will be released after
/// the task is done.
struct Job {
    task: Task,
    sender: Option<Arc<Channel<Job>>>,
    result_sender: Arc<Channel<(thread::Result<()>, Arc<Channel<Job>>)>>,
}

impl Job {
    /// Creates a new `Job` instance. Read [`Job`] for more information.
    pub(crate) fn new(
        task: Task,
        channel: Arc<Channel<Job>>,
        result_channel: Arc<Channel<(thread::Result<()>, Arc<Channel<Job>>)>>,
    ) -> Self {
        Self {
            task,
            sender: Some(channel),
            result_sender: result_channel,
        }
    }
}

impl Future for Job {
    type Output = ();

    fn poll(self: Pin<&mut Self>, mut cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        let mut future_ptr = panic::AssertUnwindSafe(this.task.future_ptr());
        let mut unwind_safe_cx = panic::AssertUnwindSafe(&mut cx);
        let handle = panic::catch_unwind(move || unsafe {
            let pinned_future = Pin::new_unchecked(&mut **future_ptr);
            pinned_future.poll(*unwind_safe_cx)
        });

        if let Ok(poll_res) = handle {
            if poll_res.is_ready() {
                local_executor().exec_global_future(async move {
                    let send_res = this
                        .result_sender
                        .send((Ok(()), this.sender.take().unwrap()))
                        .await;
                    if send_res.is_err() {
                        panic!("{BUG_MESSAGE}");
                    }
                });

                return Poll::Ready(());
            }

            Poll::Pending
        } else {
            local_executor().exec_global_future(async move {
                let send_res = this
                    .result_sender
                    .send((
                        Err(Box::new(handle.unwrap_err())),
                        this.sender.take().unwrap(),
                    ))
                    .await;
                if send_res.is_err() {
                    panic!("{BUG_MESSAGE}");
                }
            });

            Poll::Ready(())
        }
    }
}

unsafe impl Send for Job {}

/// `ExecutorPoolJoinHandle` is used to wait for the task sent to the [`ExecutorPool`]
/// to complete. It can be gotten by [`ExecutorPool::sched_future()`]. If you don't need to wait,
/// use [`sched_future_to_another_thread`].
///
/// # Panic
///
/// If not [`joined`](ExecutorPoolJoinHandle::join) before drop.
pub struct ExecutorPoolJoinHandle {
    was_joined: bool,
    channel: Arc<Channel<(thread::Result<()>, Arc<Channel<Job>>)>>,
    pool: &'static ExecutorPool,
}

impl ExecutorPoolJoinHandle {
    /// Creates a new `ExecutorPoolJoinHandle` instance.
    fn new(
        channel: Arc<Channel<(thread::Result<()>, Arc<Channel<Job>>)>>,
        pool: &'static ExecutorPool,
    ) -> Self {
        Self {
            was_joined: false,
            channel,
            pool,
        }
    }

    /// Waits for the task sent to the [`ExecutorPool`] to complete.
    pub async fn join(mut self) {
        self.was_joined = true;
        let (res, sender) = self.channel.recv().await.expect(BUG_MESSAGE);
        self.pool.senders_to_executors.push(sender);

        if let Err(err) = res {
            panic::resume_unwind(err);
        }
    }
}

impl Drop for ExecutorPoolJoinHandle {
    fn drop(&mut self) {
        assert!(
            self.was_joined,
            "ExecutorPoolJoinHandle::join() must be called! \
        If you don't want to wait result immediately, put it somewhere and join it later."
        );
    }
}

/// `EXECUTORS_FROM_POOL_IDS` contains IDs of all created executors from the pool.
///
/// It is used to prevent stopping executors from the [`pool`](EXECUTOR_POOL).
static EXECUTORS_FROM_POOL_IDS: STDMutex<BTreeSet<usize>> = STDMutex::new(BTreeSet::new());

/// Returns whether the given executor ID is in the pool.
///
/// It is used to prevent stopping executors from the [`pool`](EXECUTOR_POOL).
pub(crate) fn is_executor_id_in_pool(id: usize) -> bool {
    EXECUTORS_FROM_POOL_IDS.lock().unwrap().contains(&id)
}

/// `ExecutorPool` allows to reuse [`Executor`] instances. It is used in tests.
///
/// # Thread safety
///
/// `ExecutorPool` is thread-safe.
pub struct ExecutorPool {
    senders_to_executors: SegQueue<Arc<Channel<Job>>>,
}

/// Returns [`Config`] for creating [`Executor`] in the [`pool`](EXECUTOR_POOL).
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
    fn new_executor(&self) -> Arc<Channel<Job>> {
        let channel = Arc::new(Channel::bounded(1));
        let channel_clone = channel.clone();
        thread::spawn(move || {
            let ex = Executor::init_with_config(executor_pool_cfg());
            EXECUTORS_FROM_POOL_IDS.lock().unwrap().insert(ex.id());
            ex.run_and_block_on_global(async move {
                loop {
                    match channel_clone.recv().await {
                        Ok(job) => {
                            job.await;
                        }
                        Err(_) => {
                            // closed, it is fine
                            break;
                        }
                    }
                }
            })
            .expect(BUG_MESSAGE);
        });

        channel
    }

    /// Schedules a future to any free executor in the [`pool`](EXECUTOR_POOL).
    ///
    /// It is used to test parallelism.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::sync::atomic::AtomicUsize;
    /// use std::sync::atomic::Ordering::SeqCst;
    /// use orengine::test::{run_test_and_block_on_global, ExecutorPool};
    /// use orengine::yield_now;
    ///
    /// async fn awesome_function(atomic_to_sync_test: &AtomicUsize) {
    ///     atomic_to_sync_test.fetch_add(1, SeqCst);
    ///     yield_now().await;
    ///     atomic_to_sync_test.fetch_add(1, SeqCst);
    /// }
    ///
    /// #[cfg(test)]
    /// fn test_awesome_function() {
    ///     run_test_and_block_on_global(async {
    ///         let atomic_to_sync_test = AtomicUsize::new(0);
    ///         let mut joins = Vec::with_capacity(10);
    ///
    ///         for _ in 0..10 {
    ///             joins.push(ExecutorPool::sched_future(awesome_function(&atomic_to_sync_test)));
    ///         }
    ///
    ///         for join in joins {
    ///             join.await;
    ///         }
    ///     });
    /// }
    /// ```
    pub async fn sched_future<Fut>(future: Fut) -> ExecutorPoolJoinHandle
    where
        Fut: Future<Output = ()> + Send + 'static,
    {
        let task = Task::from_future(future, Locality::global());
        let result_channel = Arc::new(Channel::bounded(1));
        let sender = EXECUTOR_POOL
            .senders_to_executors
            .pop()
            .unwrap_or(EXECUTOR_POOL.new_executor());

        let send_res = sender
            .send(Job::new(task, sender.clone(), result_channel.clone()))
            .await;
        if send_res.is_err() {
            panic!("{BUG_MESSAGE}");
        }

        ExecutorPoolJoinHandle::new(result_channel, &EXECUTOR_POOL)
    }
}

/// Schedules a future to any free executor in the [`pool`](EXECUTOR_POOL).
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
/// ```no_run
/// use std::sync::atomic::AtomicUsize;
/// use std::sync::atomic::Ordering::SeqCst;
/// use orengine::test::{run_test_and_block_on_global, sched_future_to_another_thread};
/// use orengine::yield_now;
///
/// async fn awesome_function(atomic_to_sync_test: &AtomicUsize) {
///     atomic_to_sync_test.fetch_add(1, SeqCst);
///     yield_now().await;
///     atomic_to_sync_test.fetch_add(1, SeqCst);
/// }
///
/// #[cfg(test)]
/// fn test_awesome_function() {
///     run_test_and_block_on_global(async {
///         let atomic_to_sync_test = AtomicUsize::new(0);
///
///         for _ in 0..10 {
///             sched_future_to_another_thread(awesome_function(&atomic_to_sync_test));
///         }
///     });
/// }
/// ```
pub fn sched_future_to_another_thread<Fut>(future: Fut)
where
    Fut: Future<Output = ()> + Send + 'static,
{
    local_executor().exec_global_future(async move {
        let handle = ExecutorPool::sched_future(future).await;

        handle.join().await;
    });
}
