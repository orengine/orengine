// TODO docs

use crate::bug_message::BUG_MESSAGE;
use crate::runtime::{Config, Task};
use crate::sync::Channel;
use crate::{local_executor, Executor};
use crossbeam::queue::SegQueue;
use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex as STDMutex};
use std::task::Poll;
use std::{panic, thread};

struct Job {
    task: Task,
    sender: Option<Arc<Channel<Job>>>,
    result_sender: Arc<Channel<(thread::Result<()>, Arc<Channel<Job>>)>>,
}

impl Job {
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

pub struct ExecutorPoolJoinHandle {
    was_joined: bool,
    channel: Arc<Channel<(thread::Result<()>, Arc<Channel<Job>>)>>,
    pool: &'static ExecutorPool,
}

impl ExecutorPoolJoinHandle {
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

    pub(crate) async fn join(mut self) {
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

static EXECUTORS_FROM_POOL_IDS: STDMutex<BTreeSet<usize>> = STDMutex::new(BTreeSet::new());

pub(crate) fn is_executor_id_in_pool(id: usize) -> bool {
    EXECUTORS_FROM_POOL_IDS.lock().unwrap().contains(&id)
}

struct ExecutorPool {
    senders_to_executors: SegQueue<Arc<Channel<Job>>>,
}

fn executor_pool_cfg() -> Config {
    Config::default().disable_work_sharing()
}

impl ExecutorPool {
    pub(crate) const fn new() -> Self {
        Self {
            senders_to_executors: SegQueue::new(),
        }
    }

    pub(crate) fn new_executor(&self) -> Arc<Channel<Job>> {
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

    pub(crate) async fn sched_future<Fut>(&'static self, future: Fut) -> ExecutorPoolJoinHandle
    where
        Fut: Future<Output = ()> + Send + 'static,
    {
        let task = Task::from_future(future, 0);
        let result_channel = Arc::new(Channel::bounded(1));
        let sender = self
            .senders_to_executors
            .pop()
            .unwrap_or(self.new_executor());

        let send_res = sender
            .send(Job::new(task, sender.clone(), result_channel.clone()))
            .await;
        if send_res.is_err() {
            panic!("{BUG_MESSAGE}");
        }

        ExecutorPoolJoinHandle::new(result_channel, self)
    }
}

static EXECUTOR_POOL: ExecutorPool = ExecutorPool::new();

pub fn sched_future_to_another_thread<Fut>(future: Fut)
where
    Fut: Future<Output = ()> + Send + 'static,
{
    local_executor().exec_global_future(async move {
        let handle = EXECUTOR_POOL.sched_future(future).await;

        handle.join().await;
    });
}

// TODO r
//// TODO docs
//
// use std::future::Future;
// use std::pin::Pin;
// use std::task::Poll;
// use std::{mem, panic, thread};
// use std::sync::Arc;
// use crate::bug_message::BUG_MESSAGE;
// use crate::{local_executor, Executor};
// use crate::runtime::{Config, Task};
// use crate::sync::{Channel, WaitSend};
//
// struct Job {
//     task: Task,
//     sender: Option<Arc<Channel<Job>>>,
//     slept_by_sending_to_channel_with: Option<WaitSend<'static, (thread::Result<()>, Arc<Channel<Job>>)>>,
//     result_sender: Arc<Channel<(thread::Result<()>, Arc<Channel<Job>>)>>
// }
//
// impl Job {
//     pub(crate) fn new(
//         task: Task,
//         sender: Arc<Channel<Job>>,
//         result_sender: Arc<Channel<(thread::Result<()>, Arc<Channel<Job>>)>>,
//     ) -> Self {
//         Self {
//             task,
//             sender: Some(sender),
//             slept_by_sending_to_channel_with: None,
//             result_sender
//         }
//     }
// }
//
// macro_rules! send_to_result_channel {
//     ($this:expr, $msg:expr, $cx:expr) => {
//         let mut send_future = $this.result_sender.send($msg);
//         let send_pinned_future = unsafe {
//             Pin::new_unchecked(&mut send_future)
//         };
//
//         match send_pinned_future.poll($cx) {
//             Poll::Ready(res) => {
//                 match res {
//                     Ok(()) => {}
//                     Err(_) => {
//                         panic!("{BUG_MESSAGE}");
//                     }
//                 }
//             }
//             Poll::Pending => {
//                 let send_future_static = unsafe { mem::transmute(send_future) };
//                 $this.slept_by_sending_to_channel_with = Some(send_future_static);
//
//                 return Poll::Pending;
//             }
//         }
//
//         return Poll::Ready(());
//     };
// }
//
// impl Future for Job {
//     type Output = ();
//
//     fn poll(self: Pin<&mut Self>, mut cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
//         let this = unsafe { self.get_unchecked_mut() };
//         match this.slept_by_sending_to_channel_with.take() {
//             Some(mut fut) => {
//                 let pinned_fut = unsafe {
//                     Pin::new_unchecked(&mut fut)
//                 };
//                 match pinned_fut.poll(&mut cx) {
//                     Poll::Ready(res) => {
//                         match res {
//                             Ok(()) => {}
//                             Err(_) => {
//                                 panic!("{BUG_MESSAGE}");
//                             }
//                         }
//                     }
//                     Poll::Pending => panic!("{BUG_MESSAGE}"),
//                 }
//
//                 return Poll::Ready(());
//             }
//             None => {}
//         }
//
//         let mut future_ptr = panic::AssertUnwindSafe(this.task.future_ptr());
//         let mut unwind_safe_cx = panic::AssertUnwindSafe(&mut cx);
//         let handle = panic::catch_unwind(move || unsafe {
//             let pinned_future = Pin::new_unchecked(&mut **future_ptr);
//             pinned_future.poll(*unwind_safe_cx)
//         });
//
//         if let Ok(poll_res) = handle {
//             if poll_res.is_ready() {
//                 send_to_result_channel!(this, (Ok(()), this.sender.take().unwrap()), cx);
//             }
//
//             Poll::Pending
//         } else {
//             send_to_result_channel!(
//                 this,
//                 (Err(Box::new(handle.unwrap_err())), this.sender.take().unwrap()),
//                 cx
//             );
//         }
//     }
// }
//
// unsafe impl Send for Job {}
//
// pub struct ExecutorPoolJoinHandle {
//     was_joined: bool,
//     receiver: Arc<Channel<(thread::Result<()>, Arc<Channel<Job>>)>>,
//     pool: &'static ExecutorPool
// }
//
// impl ExecutorPoolJoinHandle {
//     fn new(
//         receiver: Arc<Channel<(thread::Result<()>, Arc<Channel<Job>>)>>,
//         pool: &'static ExecutorPool
//     ) -> Self {
//         Self {
//             was_joined: false,
//             receiver,
//             pool
//         }
//     }
//
//     pub async fn join(mut self) {
//         self.was_joined = true;
//         let task = Task::from_future(async move {
//             let (res, sender) = self.receiver.recv_timeout(std::time::Duration::from_secs(1))
//                 .await
//                 .expect(BUG_MESSAGE);
//             self.pool.senders_to_executors.lock().unwrap().push(sender);
//
//             if let Err(err) = res {
//                 panic::resume_unwind(err);
//             }
//         }, 0);
//         local_executor().exec_task_now(task);
//     }
// }
//
// impl Drop for ExecutorPoolJoinHandle {
//     fn drop(&mut self) {
//         assert!(self.was_joined, "ExecutorPoolJoinHandle::join() must be called! \
//         If you don't want to wait result immediately, put it somewhere and join it later.");
//     }
// }
//
// struct ExecutorPool {
//     senders_to_executors: std::sync::Mutex<Vec<Arc<Channel<Job>>>>,
//     config: Config
// }
//
// impl ExecutorPool {
//     pub(crate) const fn new() -> Self {
//         Self {
//             senders_to_executors: std::sync::Mutex::new(Vec::new()),
//             config: Config::default().disable_work_sharing()
//         }
//     }
//
//     pub(crate) fn new_executor(&self) -> Arc<Channel<Job>> {
//         let channel = Arc::new(Channel::bounded(1));
//         let config = self.config.clone();
//         let channel_clone = channel.clone();
//         thread::spawn(move || {
//             let ex = Executor::init_with_config(config);
//             ex.run_and_block_on_global(async move {
//                 loop {
//                     let job = channel_clone.recv_timeout(std::time::Duration::from_secs(1)).await.expect(BUG_MESSAGE);
//                     job.await;
//                 }
//             }).expect(BUG_MESSAGE);
//         });
//
//         channel
//     }
//
//     pub(crate) fn sched_future<Fut>(&'static self, future: Fut) -> ExecutorPoolJoinHandle
//     where
//         Fut: Future<Output = ()> + Send + 'static
//     {
//         let task = Task::from_future(future, 0);
//         let result_channel = Arc::new(Channel::bounded(1));
//         let result_channel_clone = result_channel.clone();
//         let mut senders_to_executors = self.senders_to_executors.lock().unwrap();
//
//         let sender = senders_to_executors
//             .pop()
//             .unwrap_or(self.new_executor());
//         drop(senders_to_executors);
//
//         local_executor().exec_global_future(async move {
//             let send_res = sender
//                 .send(Job::new(task, sender.clone(), result_channel_clone))
//                 .await;
//             if send_res.is_err() {
//                 panic!("{BUG_MESSAGE}");
//             }
//         });
//
//
//         ExecutorPoolJoinHandle::new(result_channel, self)
//     }
// }
//
// static EXECUTOR_POOL: ExecutorPool = ExecutorPool::new();
//
// #[must_use]
// pub fn sched_future_to_another_thread<Fut>(future: Fut) -> ExecutorPoolJoinHandle
// where
//     Fut: Future<Output = ()> + Send + 'static
// {
//     EXECUTOR_POOL.sched_future(future)
// }
