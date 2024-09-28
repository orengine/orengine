use std::collections::VecDeque;
use std::sync::Arc;
use std::thread;
use crate::atomic_task_queue::AtomicTaskList;
use crate::runtime::Task;

pub(crate) struct ThreadWorkerTask {
    task: Task,
    job: *mut dyn Fn()
}

unsafe impl Send for ThreadWorkerTask {}

impl ThreadWorkerTask {
    pub(crate) fn new(task: Task, job: &'static mut dyn Fn()) -> Self {
        Self {
            task,
            job
        }
    }
}

struct ThreadWorker {
    task_list: crossbeam::channel::Receiver<ThreadWorkerTask>,
    result_list: Arc<AtomicTaskList>
}

impl ThreadWorker {
    pub(crate) fn new(result_list: Arc<AtomicTaskList>) -> (
        Self,
        crossbeam::channel::Sender<ThreadWorkerTask>
    ) {
        let (
            sender,
            receiver
        ) = crossbeam::channel::unbounded();
        (
            Self {
                task_list: receiver,
                result_list
            },
            sender
        )
    }

    #[inline(always)]
    pub(crate) fn run(&mut self) {
        loop {
            match self.task_list.recv() {
                Ok(worker_task) => {
                    unsafe {
                        (&*worker_task.job)();
                        self.result_list.push(worker_task.task);
                    };
                },
                Err(_) => return
            }
        }
    }
}

pub(crate) struct LocalThreadWorkerPool {
    wait: usize,
    workers: Vec<crossbeam::channel::Sender<ThreadWorkerTask>>,
    result_list: Arc<AtomicTaskList>
}

impl LocalThreadWorkerPool {
    pub(crate) fn new(number_of_workers: usize) -> Self {
        let mut workers = Vec::with_capacity(number_of_workers);
        let result_list = Arc::new(AtomicTaskList::new());
        for _ in 0..number_of_workers {
            let (
                mut worker,
                sender
            ) = ThreadWorker::new(result_list.clone());

            thread::spawn(move || {
                worker.run();
            });

            workers.push(sender);
        }

        Self {
            wait: 0,
            workers,
            result_list
        }
    }

    #[inline(always)]
    pub(crate) fn push(&mut self, task: Task, job: &'static mut dyn Fn()) {
        let worker = self.workers[self.wait % self.workers.len()].clone();
        let send_res = worker.send(
            ThreadWorkerTask::new(task, job)
        );

        if send_res.is_err() {
            panic!("ThreadWorker is disconnected. It is only possible if the thread has panicked.");
        }

        self.wait += 1;
    }

    /// Returns whether the [`LocalThreadWorkerPool`] has work to do.
    #[inline(always)]
    pub(crate) fn poll(&mut self, other_list: &mut VecDeque<Task>) -> bool {
        if self.wait == 0 {
            return false;
        }

        let prev_len = other_list.len();
        self.result_list.take_batch(other_list, usize::MAX);
        self.wait -= other_list.len() - prev_len;

        true
    }
}