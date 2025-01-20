use crate::runtime::Task;
use crate::sync_task_queue::SyncTaskList;
use std::collections::VecDeque;
use std::sync::Arc;
use std::thread;

/// This structure represents a `worker task` that is sent
/// to the [`thread pool`](LocalThreadWorkerPool).
pub(crate) struct ThreadWorkerTask {
    /// Associated [`Task`].
    task: Task,
    /// The function to execute.
    job: *mut dyn Fn(),
}

unsafe impl Send for ThreadWorkerTask {}

impl ThreadWorkerTask {
    /// Creates a new instance of `ThreadWorkerTask`.
    pub(crate) fn new(task: Task, job: *mut dyn Fn()) -> Self {
        Self { task, job }
    }
}

/// This structure represents a worker in the [`thread pool`](LocalThreadWorkerPool).
struct ThreadWorker {
    task_channel: crossbeam::channel::Receiver<ThreadWorkerTask>,
    result_list: Arc<SyncTaskList>,
}

impl ThreadWorker {
    /// Creates a new instance of `ThreadWorker`.
    pub(crate) fn new(
        result_list: Arc<SyncTaskList>,
    ) -> (Self, crossbeam::channel::Sender<ThreadWorkerTask>) {
        let (sender, receiver) = crossbeam::channel::unbounded();
        (
            Self {
                task_channel: receiver,
                result_list,
            },
            sender,
        )
    }

    /// Runs the worker until the channel is closed.
    pub(crate) fn run(&mut self) {
        loop {
            match self.task_channel.recv() {
                Ok(worker_task) => {
                    unsafe {
                        (*worker_task.job)();
                        self.result_list.push(worker_task.task);
                    };
                }
                Err(_) => return,
            }
        }
    }
}

/// This structure represents a pool of worker threads.
pub(crate) struct LocalThreadWorkerPool {
    wait: usize,
    workers: Box<[crossbeam::channel::Sender<ThreadWorkerTask>]>,
    result_list: Arc<SyncTaskList>,
}

impl LocalThreadWorkerPool {
    /// Creates a new instance of `LocalThreadWorkerPool`.
    pub(crate) fn new(number_of_workers: usize) -> Self {
        let mut workers = Vec::with_capacity(number_of_workers);
        let result_list = Arc::new(SyncTaskList::new());
        for _ in 0..number_of_workers {
            let (mut worker, sender) = ThreadWorker::new(result_list.clone());

            thread::spawn(move || {
                worker.run();
            });

            workers.push(sender);
        }

        Self {
            wait: 0,
            workers: workers.into_boxed_slice(),
            result_list,
        }
    }

    /// Pushes a task to the [`pool`](LocalThreadWorkerPool).
    #[inline]
    pub(crate) fn push(&mut self, task: Task, job: *mut dyn Fn()) {
        let worker = &self.workers[self.wait % self.workers.len()];
        worker.send(ThreadWorkerTask::new(task, job)).expect(
            "ThreadWorker is disconnected. It is only possible if the thread has panicked.",
        );

        self.wait += 1;
    }

    /// Polls the [`pool`](LocalThreadWorkerPool) and returns whether
    /// the [`pool`](LocalThreadWorkerPool) has work to do.
    #[inline]
    pub(crate) fn poll(&mut self, other_list: &mut VecDeque<Task>) -> bool {
        if self.wait == 0 {
            return false;
        }

        let prev_len = other_list.len();
        self.result_list.pop_all_in_deque(other_list);
        self.wait -= other_list.len() - prev_len;

        true
    }
}
