//! This module contains utilities for parallel testing via
//! [`sched_future_to_another_thread`] or [`sched_future`](ExecutorPool::sched_future).
use crate::bug_message::BUG_MESSAGE;
use crate::runtime::{Config, Locality, Task};
use crate::sync::Channel;
use crate::{local_executor, Executor};
use crossbeam::queue::SegQueue;
use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Mutex as STDMutex};
use std::task::Poll;
use std::{panic, thread};
use crate::utils::Ptr;

/// `Job` is a wrapper for a task that implements the [`Future`] trait via polling
/// the associated [`Task`] and sending the result (caught panic) to the `result_sender`.
///
/// It also contains a `sender` to acquired [`Executor`] that will be released after
/// the task is done.
struct Job {
    task: Task,
    sender: Option<Ptr<Channel<Job>>>,
    result_sender: Ptr<Channel<(thread::Result<()>, Ptr<Channel<Job>>)>>,
}

impl Job {
    /// Creates a new `Job` instance. Read [`Job`] for more information.
    pub(crate) fn new(
        task: Task,
        channel: Ptr<Channel<Job>>,
        result_channel: Ptr<Channel<(thread::Result<()>, Ptr<Channel<Job>>)>>,
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
                let sender = this.sender.take().unwrap();
                local_executor().exec_global_future(async move {
                    let send_res = unsafe {
                        this
                            .result_sender
                            .as_ref()
                            .send((Ok(()), sender))
                            .await
                    };
                    if send_res.is_err() {
                        panic!("{BUG_MESSAGE}");
                    }
                });

                return Poll::Ready(());
            }

            Poll::Pending
        } else {
            let sender = this.sender.take().unwrap();
            local_executor().exec_global_future(async move {
                let send_res = unsafe {
                    this
                        .result_sender
                        .as_ref()
                        .send((
                            Err(Box::new(handle.unwrap_err())),
                            sender,
                        ))
                        .await
                };
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
    channel: Ptr<Channel<(thread::Result<()>, Ptr<Channel<Job>>)>>,
    pool: &'static ExecutorPool,
}

impl ExecutorPoolJoinHandle {
    /// Creates a new `ExecutorPoolJoinHandle` instance.
    fn new(
        channel: Ptr<Channel<(thread::Result<()>, Ptr<Channel<Job>>)>>,
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
        let (res, sender) = unsafe { self.channel.as_ref() }.recv().await.expect(BUG_MESSAGE);
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
/// It is used to prevent stopping executors from the [`pool`](ExecutorPool).
static EXECUTORS_FROM_POOL_IDS: STDMutex<BTreeSet<usize>> = STDMutex::new(BTreeSet::new());

/// `ExecutorPool` allows to reuse [`Executor`] instances. It is used in tests.
///
/// # Thread safety
///
/// `ExecutorPool` is thread-safe.
pub struct ExecutorPool {
    senders_to_executors: SegQueue<Ptr<Channel<Job>>>,
}

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
    fn new_executor(&self) -> Ptr<Channel<Job>> {
        let channel = Ptr::new(Channel::bounded(1));
        let channel_clone = channel.clone();
        thread::spawn(move || {
            let ex = Executor::init_with_config(executor_pool_cfg());
            EXECUTORS_FROM_POOL_IDS.lock().unwrap().insert(ex.id());
            ex.run_and_block_on_global(async move {
                unsafe {
                    loop {
                        match channel_clone.as_ref().recv().await {
                            Ok(job) => {
                                job.await;
                            }
                            Err(_) => {
                                // closed, it is fine
                                break;
                            }
                        }
                    }
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
    /// ```no_run
    /// use std::sync::Arc;
    /// use std::sync::atomic::AtomicUsize;
    /// use std::sync::atomic::Ordering::SeqCst;
    /// use orengine::test::{run_test_and_block_on_global, ExecutorPool};
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
    ///     run_test_and_block_on_global(async {
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
    pub async fn sched_future<Fut>(future: Fut) -> ExecutorPoolJoinHandle
    where
        Fut: Future<Output = ()> + Send + 'static,
    {
        let task = Task::from_future(future, Locality::global());
        let result_channel = Ptr::new(Channel::bounded(1));
        let sender = EXECUTOR_POOL
            .senders_to_executors
            .pop()
            .unwrap_or(EXECUTOR_POOL.new_executor());

        let send_res = unsafe {
            sender
            .as_ref()
            .send(Job::new(task, sender.clone(), result_channel.clone()))
            .await
        };
        if send_res.is_err() {
            panic!("{BUG_MESSAGE}");
        }

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
/// ```no_run
/// use std::sync::Arc;
/// use std::sync::atomic::AtomicUsize;
/// use std::sync::atomic::Ordering::SeqCst;
/// use orengine::sync::WaitGroup;
/// use orengine::test::{run_test_and_block_on_global, sched_future_to_another_thread};
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
///     run_test_and_block_on_global(async {
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
    Fut: Future<Output = ()> + Send + 'static,
{
    local_executor().exec_global_future(async move {
        let handle = ExecutorPool::sched_future(future).await;

        handle.join().await;
    });
}
