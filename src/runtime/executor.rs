use std::cell::UnsafeCell;
use std::collections::{BTreeSet, VecDeque};
use std::future::Future;
use std::intrinsics::{likely, unlikely};
use std::mem;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use crate::atomic_task_queue::AtomicTaskList;
use crate::buf::BufPool;
use crate::cfg::{config_buf_len};
use crate::end::{global_was_end, set_global_was_end, set_was_ended, was_ended};
#[cfg(test)]
use crate::end_local_thread;
use crate::io::worker::{init_local_worker, local_worker as local_io_worker, IoWorker, LOCAL_WORKER};
use crate::runtime::call::Call;
use crate::runtime::task::Task;
use crate::runtime::task_pool::TaskPool;
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
    |    1 - use let executor = Executor::init();                                            |\n\
    |    2 - use executor.spawn_local(your_future)                                           |\n\
    |            or executor.spawn_global(your_future)                                       |\n\
    |                                                                                        |\n\
    |    ATTENTION: if you want the future to finish the local runtime,                      |\n\
    |               add orengine::end_local_thread() in the end of future,                   |\n\
    |               otherwise the local runtime will never be stopped.                       |\n\
    |                                                                                        |\n\
    |    3 - use executor.run()                                                              |\n\
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

    current_call: Call,

    tasks: VecDeque<Task>,
    sleeping_tasks: BTreeSet<SleepingTask>
}

pub(crate) static FREE_WORKER_ID: AtomicUsize = AtomicUsize::new(0);

impl Executor {
    pub fn init_on_core(core_id: CoreId) -> &'static mut Executor {
        set_was_ended(false);
        set_global_was_end(false);
        BufPool::init_in_local_thread(config_buf_len());
        TaskPool::init();
        unsafe { init_local_worker() };

        unsafe {
            LOCAL_EXECUTOR.with(|executor| {
                (&mut *executor.get()).replace(Executor {
                    core_id,
                    worker_id: FREE_WORKER_ID.fetch_add(1, Ordering::Relaxed),
                    current_call: Call::default(),
                    tasks: VecDeque::new(),
                    sleeping_tasks: BTreeSet::new(),
                });
            });
        }

        unsafe { local_executor_unchecked() }
    }

    pub fn init() -> &'static mut Executor {
        let cores_id = get_core_ids();
        if cores_id.is_some() {
            Self::init_on_core(cores_id.unwrap()[0])
        } else {
            Self::init_on_core(CoreId {id: 0})
        }
    }

    pub fn worker_id(&self) -> usize {
        self.worker_id
    }

    pub fn core_id(&self) -> CoreId {
        self.core_id
    }

    #[inline(always)]
    /// # Safety
    ///
    /// * send_to must be a valid pointer to [`AtomicTaskQueue`](AtomicTaskList)
    ///
    /// * the reference must live at least as long as this state of the task
    ///
    /// * task must return [`Poll::Pending`](Poll::Pending) immediately after calling this function
    pub unsafe fn push_current_task_to(&mut self, send_to: &AtomicTaskList) {
        self.current_call = Call::PushCurrentTaskTo(send_to);
    }

    #[inline(always)]
    /// # Safety
    ///
    /// * send_to must be a valid pointer to [`AtomicTaskQueue`](AtomicTaskList)
    ///
    /// * task must return [`Poll::Pending`](Poll::Pending) immediately after calling this function
    ///
    /// * counter must be a valid pointer to [`AtomicUsize`](AtomicUsize)
    ///
    /// * the references must live at least as long as this state of the task
    pub unsafe fn push_current_task_to_and_remove_it_if_counter_is_zero(
        &mut self,
        send_to: &AtomicTaskList,
        counter: &AtomicUsize,
        order: Ordering
    ) {
        self.current_call = Call::PushCurrentTaskToAndRemoveItIfCounterIsZero(
            send_to,
            counter,
            order
        );
    }

    #[inline(always)]
    pub fn exec_task(&mut self, mut task: Task) {
        let task_ref = &mut task;
        let task_ptr = task_ref as *mut Task;
        let future = unsafe { &mut *task_ref.future_ptr };
        let waker = create_waker(task_ptr as *const ());
        let mut context = Context::from_waker(&waker);

        match unsafe { Pin::new_unchecked(future) }.as_mut().poll(&mut context) {
            Poll::Ready(()) => {
                unsafe { task.drop_future() };
            }
            Poll::Pending => {
                match mem::take(&mut self.current_call) {
                    Call::None => {}
                    Call::PushCurrentTaskTo(task_list) => {
                        unsafe { (&*task_list).push(task) }
                    }
                    Call::PushCurrentTaskToAndRemoveItIfCounterIsZero(
                        task_list,
                        counter,
                        order
                    ) => {
                        unsafe {
                            let list = &*task_list;
                            list.push(task);
                            let counter = &*counter;

                            if counter.load(order) == 0 {
                                if let Some(task) = list.pop() {
                                    self.exec_task(task);
                                } // else other thread already executed the task
                            }
                        }
                    }
                }
            }
        }
    }

    #[inline(always)]
    pub fn exec_future<F>(&mut self, future: F)
    where
        F: Future<Output=()> + 'static,
    {
        let task = Task::from_future(future);
        self.exec_task(task);
    }

    #[inline(always)]
    pub fn spawn_local<F>(&mut self, future: F)
        where
            F: Future<Output=()> + 'static,
    {
        let task = Task::from_future(future);
        unsafe { self.spawn_local_task(task); }
    }

    #[inline(always)]
    // unsafe because we can't guarantee that `task` is valid and local.
    pub unsafe fn spawn_local_task(&mut self, task: Task) {
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
    // TODO r #[allow(unused)]
    #[allow(unused)]
    pub fn spawn_global_task(&mut self, task: Task) {
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
    /// Return true, if we need to stop ([`end_local_thread`](end_local_thread) was called).
    fn background_task<W: IoWorker>(&mut self, io_worker: &mut W) -> bool {
        let has_no_work = io_worker.must_poll(Duration::ZERO);

        let instant = Instant::now();
        while let Some(sleeping_task) = self.sleeping_tasks.pop_first() {
            if sleeping_task.time_to_wake() <= instant {
                self.exec_task(sleeping_task.task());
            } else {
                let need_to_sleep = sleeping_task.time_to_wake() - instant;
                self.sleeping_tasks.insert(sleeping_task);
                if unlikely(has_no_work) {
                    const MAX_SLEEP: Duration = Duration::from_millis(100);
                    if need_to_sleep > MAX_SLEEP {
                        let _ = io_worker.must_poll(MAX_SLEEP);
                        break;
                    } else {
                        let _ = io_worker.must_poll(need_to_sleep);
                    }
                } else {
                    break;
                }
            }
        }

        if unlikely(self.tasks.capacity() > 512 && self.tasks.len() * 3 < self.tasks.capacity()) {
            self.tasks.shrink_to(self.tasks.len() * 2 + 1);
        }

        was_ended() || global_was_end()
    }

    pub fn run(&mut self) {
        let mut task_;
        let mut task;
        let io_worker = unsafe { local_io_worker() };

        loop {
            task_ = self.tasks.pop_back();
            if unlikely(task_.is_none()) {
                if unlikely(self.background_task(io_worker)) {
                    break;
                }
                continue;
            }

            task = unsafe { task_.unwrap_unchecked() };
            self.exec_task(task);
        }

        uninit_local_executor();
        set_global_was_end(false);
        set_was_ended(false);
        LOCAL_WORKER.with(|local_worker| {
            unsafe {
                *local_worker.get() = MaybeUninit::uninit();
            }
        });
        BufPool::uninit_in_local_thread();
    }

    // uses only for tests, because double memory usage of a future.
    #[cfg(test)]
    pub(crate) fn run_and_block_on<T, F>(&mut self, future: F) -> T
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
}

// uses only for tests, because double memory usage of a future.
#[cfg(test)]
pub(crate) fn create_local_executer_for_block_on<T, F>(future: F) -> T
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

    #[test_macro::test]
    fn test_spawn_local_and_exec_future() {
        async fn insert(number: u16, arr: Local<Vec<u16>>) {
            arr.get_mut().push(number);
        }

        let executor = local_executor();
        let arr = Local::new(Vec::new());

        insert(10, arr.clone()).await;
        executor.spawn_local(insert(20, arr.clone()));
        executor.spawn_local(insert(30, arr.clone()));

        yield_now().await;

        assert_eq!(&vec![10, 30, 20], arr.get()); // 30, 20 because of LIFO

        let arr = Local::new(Vec::new());

        insert(10, arr.clone()).await;
        local_executor().exec_future(insert(20, arr.clone()));
        local_executor().exec_future(insert(30, arr.clone()));

        assert_eq!(&vec![10, 20, 30], arr.get()); // 20, 30 because we don't use the queue here
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