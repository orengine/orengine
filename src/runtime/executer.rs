use std::cell::UnsafeCell;
use std::collections::{BTreeSet, VecDeque};
use std::future::Future;
use std::intrinsics::{likely, unlikely};
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::task::{Context, Poll};
use std::time::{Instant};
use crate::buf::BufPool;
use crate::cfg::{config_buf_len};
use crate::end::{set_was_ended, was_ended};
use crate::end_local_thread;
use crate::io::sys::Worker as IoWorkerSys;
use crate::io::worker::{init_local_worker, local_worker as local_io_worker, IoWorker, LOCAL_WORKER};
use crate::runtime::task::Task;
use crate::runtime::waker::create_waker;
use crate::sleep::sleeping_task::SleepingTask;
use crate::utils::{CoreId, get_core_ids};

thread_local! {
    static LOCAL_EXECUTOR: UnsafeCell<Option<Executor>> = UnsafeCell::new(None);
}

fn uninit_local_executor() {
    unsafe {
        LOCAL_EXECUTOR.with(|executor| {
            *executor.get() = None;
        })
    }
}

pub(crate) const MSG_LOCAL_EXECUTER_IS_NOT_INIT: &str ="\
    ------------------------------------------------------------------------------------------\n\
    |    Local executor is not initialized.                                                  |\n\
    |    Please initialize it first.                                                         |\n\
    |                                                                                        |\n\
    |    First way:                                                                          |\n\
    |    1 - use local_executor_init()                                                       |\n\
    |    2 - use local_executer().run_and_block_on(your_future)                              |\n\
    |                                                                                        |\n\
    |    Second way:                                                                         |\n\
    |    1 - use create_local_executer_for_block_on(your_future)                             |\n\
    |                                                                                        |\n\
    |    Third way:                                                                          |\n\
    |    1 - use local_executor_init()                                                       |\n\
    |    2 - use local_executer().spawn_local(your_future)                                   |\n\
    |            or local_executer().spawn_global(your_future)                               |\n\
    |                                                                                        |\n\
    |    ATTENTION: if you want the future to finish the local runtime,                      |\n\
    |               add orengine::end_local_thread() in the end of future,               |\n\
    |               otherwise the local runtime will never be stopped.                       |\n\
    |                                                                                        |\n\
    |    3 - use local_executer().run()                                                      |\n\
    ------------------------------------------------------------------------------------------";

#[inline(always)]
pub fn local_executor() -> &'static mut Executor {
    unsafe {
        LOCAL_EXECUTOR.with(|executor| {
            let executer_ref = &mut *executor.get();
            if likely(executer_ref.is_some()) {
                return executer_ref.as_mut().unwrap_unchecked();
            } else {
                panic!("{}", MSG_LOCAL_EXECUTER_IS_NOT_INIT);
            }
        })
    }
}

#[inline(always)]
pub unsafe fn local_executor_unchecked() -> &'static mut Executor {
    unsafe {
        LOCAL_EXECUTOR.with(|executor| {
            let executer_ref = &mut *executor.get();
            executer_ref.as_mut().unwrap_unchecked()
        })
    }
}

pub struct Executor {
    core_id: CoreId,
    worker_id: usize,

    tasks: VecDeque<Task>,
    sleeping_tasks: BTreeSet<SleepingTask>
}

pub(crate) static FREE_WORKER_ID: AtomicUsize = AtomicUsize::new(0);

impl Executor {
    pub fn init_on_core(core_id: CoreId) {
        set_was_ended(false);
        BufPool::init_in_local_thread(config_buf_len());
        unsafe { init_local_worker() };

        let executer = Executor {
            core_id,
            worker_id: FREE_WORKER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            tasks: VecDeque::new(),
            sleeping_tasks: BTreeSet::new(),
        };

        unsafe {
            LOCAL_EXECUTOR.with(|executor| {
                (&mut *executor.get()).replace(executer);
            });
        }
    }

    pub fn init() {
        let cores_id = get_core_ids();
        if cores_id.is_some() {
            Self::init_on_core(cores_id.unwrap()[0]);
        } else {
            Self::init_on_core(CoreId {id: 0})
        }
    }

    #[inline(always)]
    pub fn exec_task(mut task: Task) {
        let task_ref = &mut task;
        let task_ptr = task_ref as *mut Task;
        let future = unsafe { &mut *task_ref.future_ptr };
        let waker = create_waker(task_ptr as *const ());
        // TODO try safe context instead of waker.
        let mut context = Context::from_waker(&waker);

        match unsafe { Pin::new_unchecked(future) }.as_mut().poll(&mut context) {
            Poll::Ready(()) => {
                unsafe { task.drop_future() };
            }
            Poll::Pending => {}
        }
    }

    #[inline(always)]
    pub fn exec_future<F>(&mut self, future: F)
    where
        F: Future<Output=()> + 'static,
    {
        let task = Task::from_future(future);
        Self::exec_task(task);
    }

    #[inline(always)]
    pub fn spawn_local<F>(&mut self, future: F)
        where
            F: Future<Output=()> + 'static,
    {
        let task = Task::from_future(future);
        self.tasks.push_back(task);
    }

    #[inline(always)]
    // TODO r #[allow(unused)]
    #[allow(unused)]
    pub fn spawn_global<F>(&mut self, future: F)
    where
        F: Future<Output=()> + Send + 'static,
    {
        todo!()
    }

    #[inline(always)]
    pub fn local_queue(&mut self) -> &mut VecDeque<Task> {
        &mut self.tasks
    }

    #[inline(always)]
    pub fn sleeping_tasks(&mut self) -> &mut BTreeSet<SleepingTask> {
        &mut self.sleeping_tasks
    }

    #[inline(always)]
    /// Return true, if we need to stop ([`end_local_thread`](crate::end::end_local_thread) was called).
    fn background_task(&mut self, io_worker: &mut IoWorkerSys) -> bool {
        io_worker.must_poll();

        let instant = Instant::now();
        while let Some(sleeping_task) = self.sleeping_tasks.pop_first() {
            if sleeping_task.time_to_wake() <= instant {
                Self::exec_task(sleeping_task.task());
            } else {
                self.sleeping_tasks.insert(sleeping_task);
                break;
            }
        }

        if unlikely(self.tasks.len() * 3 < self.tasks.capacity()) {
            if self.tasks.capacity() != 1 {
                self.tasks.shrink_to(self.tasks.len() * 2 + 1);
            }
        }

        was_ended()
    }

    pub fn run(&mut self) {
        let mut task_;
        let mut task;
        let mut exec_times = 0;
        let io_worker = unsafe { local_io_worker() };

        loop {
            task_ = self.tasks.pop_back();
            if unlikely(task_.is_none()) {
                if unlikely(self.background_task(io_worker)) {
                    break;
                }
                exec_times = 0;
                continue;
            }

            if unlikely(exec_times == 60) {
                if unlikely(self.background_task(io_worker)) {
                    break;
                }
                exec_times = 0;
            } else {
                exec_times += 1;
            }

            task = unsafe { task_.unwrap_unchecked() };
            Self::exec_task(task);
        }

        uninit_local_executor();
        LOCAL_WORKER.with(|local_worker| {
            unsafe {
                *local_worker.get() = MaybeUninit::uninit();
            }
        });
        BufPool::uninit_in_local_thread();
    }

    pub fn run_and_block_on<T, F>(&mut self, future: F) -> T
        where
            T: 'static,
            F: Future<Output=T> + 'static,
    {
        let mut res = MaybeUninit::uninit();
        let res_ptr: *mut T = res.as_mut_ptr();
        self.exec_future(async move {
            unsafe { res_ptr.write(future.await) };
            end_local_thread();
        });
        self.run();
        unsafe { res.assume_init() }
    }

    pub fn worker_id(&self) -> usize {
        self.worker_id
    }

    pub fn core_id(&self) -> CoreId {
        self.core_id
    }
}

pub fn local_core_id() -> CoreId {
    local_executor().core_id
}

pub fn local_worker_id() -> usize {
    local_executor().worker_id
}

pub fn create_local_executer_for_block_on<T, F>(future: F) -> T
    where
        T: 'static,
        F: Future<Output=T> + 'static,
{
    Executor::init();
    local_executor().run_and_block_on(future)
}

#[cfg(test)]
mod tests {
    use crate::local::Local;
    use crate::yield_now;
    use super::*;

    #[test]
    fn test_spawn_local_and_exec_future() {
        async fn insert(number: u16, arr: Local<Vec<u16>>) {
            arr.get_mut().push(number);
        }

        create_local_executer_for_block_on(async {
            let executer = local_executor();
            let arr = Local::new(Vec::new());

            insert(10, arr.clone()).await;
            executer.spawn_local(insert(20, arr.clone()));
            executer.spawn_local(insert(30, arr.clone()));

            yield_now().await;

            assert_eq!(&vec![10, 30, 20], arr.get()); // 30, 20 because of LIFO
        });

        create_local_executer_for_block_on(async {
            let executer = local_executor();
            let arr = Local::new(Vec::new());

            insert(10, arr.clone()).await;
            executer.exec_future(insert(20, arr.clone()));
            executer.exec_future(insert(30, arr.clone()));

            assert_eq!(&vec![10, 20, 30], arr.get()); // 20, 30 because we don't use the queue here
            // (code is executed in this function sequentially)
        });
    }

    #[test]
    fn test_run_and_block_on() {
        async fn async_42() -> u32 {
            42
        }

        Executor::init();
        assert_eq!(42, local_executor().run_and_block_on(async_42()));
    }
}