use std::cell::UnsafeCell;
use std::collections::{BTreeSet, VecDeque};
use std::future::Future;
use std::mem;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

use crate::check_task_local_safety;
use crate::io::sys::WorkerSys;
use crate::io::worker::{get_local_worker_ref, init_local_worker, uninit_local_worker, IoWorker};
use crate::runtime::call::Call;
use crate::runtime::config::{Config, ValidConfig};
use crate::runtime::end_local_thread_and_write_into_ptr::EndLocalThreadAndWriteIntoPtr;
use crate::runtime::global_state::{register_local_executor, SubscribedState};
use crate::runtime::local_thread_pool::LocalThreadWorkerPool;
use crate::runtime::task::{Task, TaskPool};
use crate::runtime::waker::create_waker;
use crate::runtime::{get_core_id_for_executor, SharedExecutorTaskList};
use crate::sleep::sleeping_task::SleepingTask;
use crate::sync_task_queue::SyncTaskList;
use crate::utils::CoreId;
use crossbeam::utils::CachePadded;
use fastrand::Rng;

macro_rules! shrink {
    ($list:expr) => {
        if $list.capacity() > 512 && $list.len() * 3 < $list.capacity() {
            let new_len = $list.len() * 2 + 1;
            $list.shrink_to(new_len);
        }
    };
}

thread_local! {
    /// Thread local [`Executor`]. So, it is lockless.
    pub(crate) static LOCAL_EXECUTOR: UnsafeCell<Option<Executor>> = UnsafeCell::new(None);
}

/// Returns the thread-local executor wrapped in an [`Option`].
///
/// It is `None` if the executor is not initialized.
fn get_local_executor_ref() -> &'static mut Option<Executor> {
    LOCAL_EXECUTOR.with(|local_executor| unsafe { &mut *local_executor.get() })
}

/// Change the state of local thread to pre-initialized.
fn uninit_local_executor() {
    *get_local_executor_ref() = None;
    uninit_local_worker();
}

/// Message that prints out when local executor is not initialized
/// but [`local_executor()`](local_executor) is called.
#[cfg(debug_assertions)]
pub(crate) const MSG_LOCAL_EXECUTOR_IS_NOT_INIT: &str = "\
------------------------------------------------------------------------------------------
|    Local executor is not initialized.                                                  |
|    Please initialize it first.                                                         |
|                                                                                        |
|    First way:                                                                          |
|    1 - let executor = Executor::init();                                                |
|    2 - executor.run_with_global_future(your_future) or                                 |
|        executor.run_with_local_future(your_future)                                     |
|                                                                                        |
|    ATTENTION:                                                                          |
|    To stop the executor, save in the start of the future local_executor().id()         |
|    and call orengine::stop_executor(executor_id), or                                   |
|    call orengine::stop_all_executors to stop the entire runtime.                       |
|                                                                                        |
|    Second way:                                                                         |
|    1 - let executor = Executor::init();                                                |
|    2 - executor.spawn_local(your_future) or                                            |
|        executor.spawn_global(your_future)                                              |
|    3 - executor.run()                                                                  |
|                                                                                        |
|    ATTENTION:                                                                          |
|    To stop the executor, save in the start of the future local_executor().id()         |
|    and call orengine::stop_executor(executor_id), or                                   |
|    call orengine::stop_all_executors to stop the entire runtime.                       |
|                                                                                        |
|    Third way:                                                                          |
|    1 - let executor = Executor::init();                                                |
|    2 - executor.run_and_block_on_local(your_future) or                                 |
|        executor.run_and_block_on_global(your_future)                                   |
|                                                                                        |
|        This will block the current thread executor until the future completes.         |
|        And after the future completes, the executor will be stopped.                   |
------------------------------------------------------------------------------------------";

/// Returns the [`Executor`] that is running in the current thread.
///
/// # Panics
///
/// If the local executor is not initialized.
///
/// # Undefined Behavior
///
/// If the local executor is not initialized and the program is in `release` mode.
///
/// Read [`MSG_LOCAL_EXECUTOR_IS_NOT_INIT`] for more details.
#[inline(always)]
pub fn local_executor() -> &'static mut Executor {
    #[cfg(debug_assertions)]
    {
        get_local_executor_ref()
            .as_mut()
            .expect(MSG_LOCAL_EXECUTOR_IS_NOT_INIT)
    }

    #[cfg(not(debug_assertions))]
    unsafe {
        crate::runtime::executor::get_local_executor_ref()
            .as_mut()
            .unwrap_unchecked()
    }
}

/// Returns the [`Executor`] that is running in the current thread.
///
/// # Undefined Behavior
///
/// If the local executor is not initialized.
#[inline(always)]
pub unsafe fn local_executor_unchecked() -> &'static mut Executor {
    unsafe { get_local_executor_ref().as_mut().unwrap_unchecked() }
}

/// The executor that runs futures in the current thread.
///
/// # The difference between `local` and `global` task and futures
///
/// - `local` tasks and futures are executed only in the current thread.
/// It means that the tasks and futures can't be moved between threads.
/// It allows to use `Shared-nothing architecture` that means that in these futures and tasks
/// you can use [`Local`](crate::Local) and `local primitives of synchronization`.
/// Using `local` types can improve performance.
///
/// - `global` tasks and futures can be moved between threads.
/// It allows to use `global primitives of synchronization` and to `share work`.
///
/// # Share work
///
/// When a number of global tasks in the executor is become greater
/// than [`runtime::Config.work_sharing_level`](Config::set_work_sharing_level) `Executor`
/// shares the half of work with other executors.
///
/// When `Executor` has no work, it tries to take tasks from other executors.
pub struct Executor {
    core_id: CoreId,
    executor_id: usize,
    config: ValidConfig,
    subscribed_state: SubscribedState,
    rng: Rng,

    local_tasks: VecDeque<Task>,
    global_tasks: VecDeque<Task>,
    shared_tasks_list: Option<Arc<SharedExecutorTaskList>>,

    exec_series: usize,
    local_worker: &'static mut Option<WorkerSys>,
    thread_pool: LocalThreadWorkerPool,
    current_call: Call,
    local_sleeping_tasks: BTreeSet<SleepingTask>,
}

/// The next id of the executor. It is used to generate the unique executor id.
///
/// Use [`FREE_EXECUTOR_ID.fetch_add(1, Ordering::Relaxed)`](AtomicUsize::fetch_add)
/// to get the unique id.
pub(crate) static FREE_EXECUTOR_ID: AtomicUsize = AtomicUsize::new(0);

/// `MAX_NUMBER_OF_TASKS_TAKEN` is the maximum number of tasks that can be taken from
/// other executors shared lists.
const MAX_NUMBER_OF_TASKS_TAKEN: usize = 16;

impl Executor {
    /// Initializes the executor in the current thread with provided config on the given core.
    ///
    /// # Panics
    ///
    /// If the local executor is already initialized.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::runtime::Config;
    /// use orengine::Executor;
    /// use orengine::utils::get_core_ids;
    ///
    /// fn main() {
    ///     let cores = get_core_ids().unwrap();
    ///     let ex = Executor::init_on_core_with_config(cores[0], Config::default());
    ///
    ///     ex.spawn_local(async {
    ///         println!("Hello, world!");
    ///     });
    ///     ex.run();
    /// }
    /// ```
    pub fn init_on_core_with_config(core_id: CoreId, config: Config) -> &'static mut Executor {
        if get_local_executor_ref().is_some() {
            panic!("There is already an initialized executor in the current thread!");
        }

        let valid_config = config.validate();
        crate::utils::core::set_for_current(core_id);
        let executor_id = FREE_EXECUTOR_ID.fetch_add(1, Ordering::Relaxed);
        TaskPool::init();
        let (shared_tasks, global_tasks_list_cap) = match valid_config.is_work_sharing_enabled() {
            true => (
                Some(Arc::new(SharedExecutorTaskList::new(executor_id))),
                MAX_NUMBER_OF_TASKS_TAKEN,
            ),
            false => (None, 0),
        };
        let number_of_thread_workers = valid_config.number_of_thread_workers;

        unsafe {
            if let Some(io_config) = valid_config.io_worker_config {
                init_local_worker(io_config);
            }
            *get_local_executor_ref() = Some(Executor {
                core_id,
                executor_id,
                config: valid_config,
                subscribed_state: SubscribedState::new(),
                rng: Rng::new(),

                local_tasks: VecDeque::new(),
                global_tasks: VecDeque::with_capacity(global_tasks_list_cap),
                shared_tasks_list: shared_tasks,

                current_call: Call::default(),
                exec_series: 0,
                local_worker: get_local_worker_ref(),
                thread_pool: LocalThreadWorkerPool::new(number_of_thread_workers),
                local_sleeping_tasks: BTreeSet::new(),
            });

            register_local_executor();

            local_executor_unchecked()
        }
    }

    /// Initializes the executor in the current thread on the given core.
    ///
    /// # Panics
    ///
    /// If the local executor is already initialized.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::Executor;
    /// use orengine::utils::get_core_ids;
    ///
    /// fn main() {
    ///     let cores = get_core_ids().unwrap();
    ///     let ex = Executor::init_on_core(cores[0]);
    ///
    ///     ex.spawn_local(async {
    ///         println!("Hello, world!");
    ///     });
    ///     ex.run();
    /// }
    /// ```
    pub fn init_on_core(core_id: CoreId) -> &'static mut Executor {
        Self::init_on_core_with_config(core_id, Config::default())
    }

    /// Initializes the executor in the current thread with provided config.
    ///
    /// # Panics
    ///
    /// If the local executor is already initialized.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::runtime::Config;
    /// use orengine::Executor;
    ///
    /// fn main() {
    ///     let ex = Executor::init_with_config(Config::default());
    ///
    ///     ex.spawn_local(async {
    ///         println!("Hello, world!");
    ///     });
    ///     ex.run();
    /// }
    /// ```
    pub fn init_with_config(config: Config) -> &'static mut Executor {
        Self::init_on_core_with_config(get_core_id_for_executor(), config)
    }

    /// Initializes the executor in the current thread.
    ///
    /// # Panics
    ///
    /// If the local executor is already initialized.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::Executor;
    ///
    /// fn main() {
    ///     let ex = Executor::init();
    ///
    ///     ex.spawn_local(async {
    ///         println!("Hello, world!");
    ///     });
    ///     ex.run();
    /// }
    /// ```
    pub fn init() -> &'static mut Executor {
        Self::init_on_core(get_core_id_for_executor())
    }

    /// Returns the id of the executor.
    pub fn id(&self) -> usize {
        self.executor_id
    }

    /// Add a task to the beginning of the local lifo queue.
    #[inline(always)]
    pub(crate) fn add_task_at_the_start_of_lifo_local_queue(&mut self, task: Task) {
        debug_assert!(task.is_local());

        self.local_tasks.push_front(task);
    }

    /// Add a task to the beginning of the global lifo queue.
    #[inline(always)]
    pub(crate) fn add_task_at_the_start_of_lifo_global_queue(&mut self, task: Task) {
        debug_assert!(!task.is_local());

        self.global_tasks.push_back(task);
    }

    /// Returns a reference to the subscribed state of the executor.
    pub(crate) fn subscribed_state(&self) -> &SubscribedState {
        &self.subscribed_state
    }

    /// Returns a mutable reference to the subscribed state of the executor.
    pub(crate) fn subscribed_state_mut(&mut self) -> &mut SubscribedState {
        &mut self.subscribed_state
    }

    /// Returns the core id on which the executor is running.
    pub fn core_id(&self) -> CoreId {
        self.core_id
    }

    /// Returns the [`config`](Config) of the executor.
    pub fn config(&self) -> Config {
        Config::from(&self.config)
    }

    /// Returns a reference to the shared tasks list of the executor.
    pub(crate) fn shared_task_list(&self) -> Option<&Arc<SharedExecutorTaskList>> {
        self.shared_tasks_list.as_ref()
    }

    /// Sets the buffer capacity of the executor.
    pub(crate) fn set_config_buffer_cap(&mut self, buffer_len: usize) {
        self.config.buffer_cap = buffer_len;
    }

    /// Invokes [`Call::PushCurrentTaskTo`]. Use it only if you know what you are doing.
    ///
    /// Read [`Call`] for more details.
    ///
    /// # Safety
    ///
    /// * send_to must be a valid pointer to [`SyncTaskQueue`](SyncTaskList)
    ///
    /// * the reference must live at least as long as this state of the task
    ///
    /// * task must return [`Poll::Pending`](Poll::Pending) immediately after calling this function
    ///
    /// * calling task must be global (else you don't need any [`Calls`](Call))
    #[inline(always)]
    pub unsafe fn push_current_task_to(&mut self, send_to: &SyncTaskList) {
        debug_assert!(self.current_call.is_none());
        self.current_call = Call::PushCurrentTaskTo(send_to);
    }

    /// Invokes [`Call::PushCurrentTaskAtTheStartOfLIFOGlobalQueue`]. Use it only if you know what you are doing.
    ///
    /// Read [`Call`] for more details.
    ///
    /// # Safety
    ///
    /// * the reference must live at least as long as this state of the task
    ///
    /// * task must return [`Poll::Pending`](Poll::Pending) immediately after calling this function
    ///
    /// * calling task must be global (else you don't need any [`Calls`](Call))
    #[inline(always)]
    pub unsafe fn push_current_task_at_the_start_of_lifo_global_queue(&mut self) {
        debug_assert!(self.current_call.is_none());
        self.current_call = Call::PushCurrentTaskAtTheStartOfLIFOGlobalQueue;
    }

    /// Invokes [`Call::PushCurrentTaskToAndRemoveItIfCounterIsZero`].
    /// Use it only if you know what you are doing.
    ///
    /// Read [`Call`] for more details.
    ///
    /// # Safety
    ///
    /// * send_to must be a valid pointer to [`SyncTaskQueue`](SyncTaskList)
    ///
    /// * task must return [`Poll::Pending`](Poll::Pending) immediately after calling this function
    ///
    /// * counter must be a valid pointer to [`AtomicUsize`](AtomicUsize)
    ///
    /// * the references must live at least as long as this state of the task
    ///
    /// * calling task must be global (else you don't need any [`Calls`](Call))
    #[inline(always)]
    pub unsafe fn push_current_task_to_and_remove_it_if_counter_is_zero(
        &mut self,
        send_to: &SyncTaskList,
        counter: &AtomicUsize,
        order: Ordering,
    ) {
        debug_assert!(self.current_call.is_none());
        self.current_call =
            Call::PushCurrentTaskToAndRemoveItIfCounterIsZero(send_to, counter, order);
    }

    /// Invokes [`Call::ReleaseAtomicBool`]. Use it only if you know what you are doing.
    ///
    /// Read [`Call`] for more details.
    ///
    /// # Safety
    ///
    /// * atomic_bool must be a valid pointer to [`AtomicBool`](AtomicBool)
    ///
    /// * the [`AtomicBool`] must live at least as long as this state of the task
    ///
    /// * task must return [`Poll::Pending`](Poll::Pending) immediately after calling this function
    ///
    /// * calling task must be global (else you don't need any [`Calls`](Call))
    #[inline(always)]
    pub unsafe fn release_atomic_bool(&mut self, atomic_bool: *const CachePadded<AtomicBool>) {
        debug_assert!(self.current_call.is_none());
        self.current_call = Call::ReleaseAtomicBool(atomic_bool);
    }

    /// Invokes [`Call::PushFnToThreadPool`]. Use it only if you know what you are doing.
    ///
    /// Read [`Call`] for more details.
    ///
    /// # Safety
    ///
    /// * the [`Fn`] must live at least as long as this state of the task.
    ///
    /// * task must return [`Poll::Pending`](Poll::Pending) immediately after calling this function
    ///
    /// * calling task must be global (else you don't need any [`Calls`](Call))
    #[inline(always)]
    pub unsafe fn push_fn_to_thread_pool(&mut self, f: &'static mut dyn Fn()) {
        debug_assert!(self.current_call.is_none());
        self.current_call = Call::PushFnToThreadPool(f)
    }

    /// Executes a provided [`task`](Task) in the current [`executor`](Executor).
    ///
    /// # Attention
    ///
    /// Execute [`tasks`](Task) only by this method!
    #[inline(always)]
    pub fn exec_task(&mut self, task: Task) {
        if self.exec_series >= 106 {
            self.exec_series = 0;
            self.spawn_local_task(task);
            return;
        }

        self.exec_task_now(task);
    }

    /// Executes a provided [`task`](Task) in the current [`executor`](Executor).
    ///
    /// # The difference between `exec_task_now` and [`exec_task`](Executor::exec_task)
    ///
    /// If the stack of calls is too big, [`exec_task`](Executor::exec_task)
    /// spawns a new task and returns.
    /// Otherwise, [`exec_task_now`](Executor::exec_task_now) executes the task any way.
    ///
    /// # Attention
    ///
    /// Execute [`tasks`](Task) only by this method!
    #[inline(always)]
    pub(crate) fn exec_task_now(&mut self, mut task: Task) {
        self.exec_series += 1;

        let task_ref = &mut task;
        let task_ptr = task_ref as *mut Task;
        let future = unsafe { &mut *task_ref.future_ptr() };
        check_task_local_safety!(task);
        let waker = create_waker(task_ptr as *const ());
        let mut context = Context::from_waker(&waker);

        match unsafe { Pin::new_unchecked(future) }
            .as_mut()
            .poll(&mut context)
        {
            Poll::Ready(()) => {
                debug_assert_eq!(self.current_call, Call::None);
                unsafe { task.release_future() };
            }
            Poll::Pending => {
                let old_call = mem::take(&mut self.current_call);
                match old_call {
                    Call::None => {}
                    Call::PushCurrentTaskAtTheStartOfLIFOGlobalQueue => {
                        self.global_tasks.push_front(task);
                    }
                    Call::PushCurrentTaskTo(task_list) => unsafe { (&*task_list).push(task) },
                    Call::PushCurrentTaskToAndRemoveItIfCounterIsZero(
                        task_list,
                        counter,
                        order,
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
                    Call::ReleaseAtomicBool(atomic_ptr) => {
                        let atomic_ref = unsafe { &*atomic_ptr };
                        atomic_ref.store(false, Ordering::Release);
                    }
                    Call::PushFnToThreadPool(f) => {
                        debug_assert_ne!(
                            self.config.number_of_thread_workers, 0,
                            "try to use thread pool with 0 workers"
                        );

                        self.thread_pool.push(task, f);
                    }
                }
            }
        }
    }

    /// Creates a `local` [`task`](Task) from a provided [`future`](Future)
    /// and executes it in the current [`executor`](Executor).
    ///
    /// # Attention
    ///
    /// Execute [`Future`] only by this method!
    #[inline(always)]
    pub fn exec_local_future<F>(&mut self, future: F)
    where
        F: Future<Output = ()>,
    {
        let task = Task::from_future(future, 1);
        self.exec_task(task);
    }

    /// Creates a `global` [`task`](Task) from a provided [`future`](Future)
    /// and executes it in the current [`executor`](Executor).
    ///
    /// # Attention
    ///
    /// Execute [`Future`] only by this method!
    #[inline(always)]
    pub fn exec_global_future<F>(&mut self, future: F)
    where
        F: Future<Output = ()> + Send,
    {
        let task = Task::from_future(future, 0);
        self.exec_task(task);
    }

    /// Creates a local [`task`](Task) from a provided [`future`](Future) and enqueues it.
    ///
    /// # Attention
    ///
    /// This function enqueues it at the end of the queue of local tasks, but it is `LIFO`.
    ///
    /// # The difference between global and local tasks
    ///
    /// Read it in [`Executor`].
    #[inline(always)]
    pub fn spawn_local<F>(&mut self, future: F)
    where
        F: Future<Output = ()>,
    {
        let task = Task::from_future(future, 1);
        self.spawn_local_task(task);
    }

    /// Enqueues a local [`task`](Task).
    ///
    /// # Attention
    ///
    /// This function enqueues it at the end of the queue of local tasks, but it is `LIFO`.
    ///
    /// # The difference between global and local tasks
    ///
    /// Read it in [`Executor`].
    #[inline(always)]
    pub fn spawn_local_task(&mut self, task: Task) {
        self.local_tasks.push_back(task);
    }

    /// Creates a global [`task`](Task) from a provided [`future`](Future) and enqueues it.
    ///
    /// # Attention
    ///
    /// This function enqueues it at the end of the queue of global tasks, but it is `LIFO`.
    ///
    /// # The difference between global and local tasks
    ///
    /// Read it in [`Executor`].
    #[inline(always)]
    pub fn spawn_global<F>(&mut self, future: F)
    where
        F: Future<Output = ()> + Send,
    {
        let task = Task::from_future(future, 0);
        self.spawn_global_task(task);
    }

    /// Enqueues a global [`task`](Task).
    ///
    /// # Attention
    ///
    /// This function enqueues it at the end of the queue of global tasks, but it is `LIFO`.
    ///
    /// # The difference between global and local tasks
    ///
    /// Read it in [`Executor`].
    #[inline(always)]
    pub fn spawn_global_task(&mut self, task: Task) {
        match self.config.is_work_sharing_enabled() {
            true => {
                if self.global_tasks.len() <= self.config.work_sharing_level {
                    self.global_tasks.push_back(task);
                } else {
                    if let Some(mut shared_tasks_list) =
                        unsafe { self.shared_tasks_list.as_ref().unwrap_unchecked().as_vec() }
                    {
                        let number_of_shared = (self.config.work_sharing_level >> 1).min(1);
                        for task in self.global_tasks.drain(..number_of_shared) {
                            shared_tasks_list.push(task);
                        }
                    } else {
                        self.global_tasks.push_back(task);
                    }
                }
            }
            false => {
                self.global_tasks.push_back(task);
            }
        }
    }

    /// Returns a reference to the local tasks queue.
    #[inline(always)]
    pub fn local_queue(&mut self) -> &mut VecDeque<Task> {
        &mut self.local_tasks
    }

    /// Returns a reference to the `sleeping_tasks`.
    #[inline(always)]
    pub(crate) fn sleeping_tasks(&mut self) -> &mut BTreeSet<SleepingTask> {
        &mut self.local_sleeping_tasks
    }

    /// Tries to take a batch of tasks from the global tasks queue if needed.
    #[inline(always)]
    fn take_work_if_needed(&mut self) {
        if self.global_tasks.len() >= MAX_NUMBER_OF_TASKS_TAKEN {
            return;
        }
        if let Some(shared_task_list) = self.shared_tasks_list.as_mut() {
            if let Some(mut shared_task_list) = shared_task_list.as_vec() {
                shrink!(shared_task_list);
                if shared_task_list.is_empty() {
                    return;
                }
            }

            let lists = unsafe { self.subscribed_state.tasks_lists() };
            if lists.is_empty() {
                return;
            }

            let max_number_of_tries = self.rng.usize(0..lists.len()) + 1;

            for i in 0..max_number_of_tries {
                let list = unsafe { lists.get_unchecked(i) };
                let limit = MAX_NUMBER_OF_TASKS_TAKEN - self.global_tasks.len();
                if limit == 0 {
                    return;
                }

                list.take_batch(&mut self.global_tasks, limit);
            }
        }
    }

    /// Does background work like:
    ///
    /// - polls blocking worker
    ///
    /// - polls io worker
    ///
    /// - takes works if needed
    ///
    /// - checks sleeping tasks
    ///
    /// Returns true, if we need to stop ([`end_local_thread`](end_local_thread)
    /// was called or [`end`](crate::runtime::end::end)).
    #[inline(always)]
    fn background_task(&mut self) -> bool {
        self.subscribed_state.check_subscription(self.executor_id);
        if self.subscribed_state.is_stopped() {
            return true;
        }

        self.exec_series = 0;
        self.take_work_if_needed();
        self.thread_pool.poll(&mut self.local_tasks);
        match self.local_worker {
            Some(io_worker) => io_worker.must_poll(),
            None => true,
        };

        if self.local_sleeping_tasks.len() > 0 {
            let instant = Some(Instant::now());
            while let Some(sleeping_task) = self.local_sleeping_tasks.pop_first() {
                if sleeping_task.time_to_wake() <= unsafe { instant.unwrap_unchecked() } {
                    let task = sleeping_task.task();
                    if task.is_local() {
                        self.exec_task(task);
                    } else {
                        self.spawn_global_task(task);
                    }
                } else {
                    self.local_sleeping_tasks.insert(sleeping_task);
                    break;
                }
            }
        }

        shrink!(self.local_tasks);
        shrink!(self.global_tasks);

        false
    }
}

macro_rules! generate_run_and_block_on_function {
    ($func:expr, $future:expr, $executor:expr) => {{
        let mut res = None;
        let static_future = EndLocalThreadAndWriteIntoPtr::new(&mut res, $future);
        $func($executor, static_future);
        $executor.run();
        res.ok_or(
            "The process has been stopped by stop_all_executors \
                or stop_executor not in block_on future.",
        )
    }};
}

// region run

impl Executor {
    /// Runs the executor.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::{Executor, stop_executor, sleep};
    /// use std::time::Duration;
    ///
    /// fn main() {
    ///     let mut executor = Executor::init();
    ///     let id = executor.id();
    ///
    ///     executor.spawn_local(async move {
    ///         println!("Hello from an async runtime!");
    ///         sleep(Duration::from_secs(3)).await;
    ///         stop_executor(id); // stops the executor
    ///     });
    ///     executor.run();
    ///
    ///     println!("Hello from a sync runtime after at least 3 seconds");
    /// }
    /// ```
    pub fn run(&mut self) {
        let mut task;
        // A round is a number of tasks that must be completed before the next background_work call.
        // It is needed to avoid case like:
        //   Task with yield -> repeat this task -> repeat this task -> ...
        //
        // So it works like:
        //   Round 1 -> background work -> round 2  -> ...
        let mut number_of_local_tasks_in_this_round = self.local_tasks.len();
        let mut number_of_global_tasks_in_this_round = self.global_tasks.len();

        loop {
            for _ in 0..number_of_local_tasks_in_this_round {
                task = unsafe { self.local_tasks.pop_back().unwrap_unchecked() };
                self.exec_task(task);
            }

            for _ in 0..number_of_global_tasks_in_this_round {
                task = unsafe { self.global_tasks.pop_back().unwrap_unchecked() };
                self.exec_task(task);
            }

            if let Some(shared_tasks_list) = self.shared_tasks_list.as_ref() {
                if self.global_tasks.len() < self.config.work_sharing_level {
                    let prev_len = self.global_tasks.len();
                    let to_take =
                        (self.config.work_sharing_level - prev_len).min(MAX_NUMBER_OF_TASKS_TAKEN);
                    shared_tasks_list.take_batch(&mut self.global_tasks, to_take);

                    let taken = self.global_tasks.len() - prev_len;
                    for _ in 0..taken {
                        let task = unsafe { self.global_tasks.pop_back().unwrap_unchecked() };
                        self.exec_task(task);
                    }
                }
            }

            if self.background_task() {
                break;
            }

            number_of_local_tasks_in_this_round = self.local_tasks.len();
            number_of_global_tasks_in_this_round = self.global_tasks.len();
        }

        uninit_local_executor();
    }

    /// Runs the executor with a local task.
    ///
    /// # The difference between global and local tasks
    ///
    /// Read it in [`Executor`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::{Executor, stop_executor, sleep, Local};
    /// use std::time::Duration;
    ///
    /// fn main() {
    ///     let mut executor = Executor::init();
    ///     let id = executor.id();
    ///     let local_msg = Local::new("Hello from an async runtime!"); // bad example of usage Local,
    ///     // but you can use Local, because here we use a local task.
    ///
    ///     executor.run_with_local_future(async move {
    ///         println!("{}" ,local_msg);
    ///         sleep(Duration::from_secs(3)).await;
    ///         stop_executor(id); // stops the executor
    ///     });
    ///
    ///     println!("Hello from a sync runtime after at least 3 seconds");
    /// }
    /// ```
    pub fn run_with_local_future<Fut: Future<Output = ()>>(&mut self, future: Fut) {
        self.spawn_local(future);
        self.run();
    }

    /// Runs the executor with a global task.
    ///
    /// # The difference between global and local tasks
    ///
    /// Read it in [`Executor`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::{Executor, stop_executor, sleep, Local};
    /// use std::time::Duration;
    ///
    /// fn main() {
    ///     let mut executor = Executor::init();
    ///     let id = executor.id();
    ///
    ///     executor.run_with_global_future(async move {
    ///         println!("Hello from an async runtime!");
    ///         sleep(Duration::from_secs(3)).await;
    ///         stop_executor(id); // stops the executor
    ///     });
    ///
    ///     println!("Hello from a sync runtime after at least 3 seconds");
    /// }
    /// ```
    pub fn run_with_global_future<Fut: Future<Output = ()> + Send>(&mut self, future: Fut) {
        self.spawn_global(future);
        self.run();
    }

    /// Runs the executor with a local task and blocks on it. The executor will be stopped
    /// after the task completes.
    ///
    /// # The difference between global and local tasks
    ///
    /// Read it in [`Executor`].
    ///
    /// # Returns
    ///
    /// It returns `Err(&'static msg)` if undefined behavior happened or `Ok(T)` if everything is ok.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::{Executor, stop_executor, sleep, Local};
    /// use std::time::Duration;
    ///
    /// fn main() {
    ///     let mut executor = Executor::init();
    ///     let id = executor.id();
    ///     let local_msg = Local::new("Hello from an async runtime!"); // bad example of usage Local,
    ///     // but you can use Local, because here we use a local task.
    ///
    ///     let res = executor.run_and_block_on_local(async move {
    ///         println!("{}" ,local_msg);
    ///         sleep(Duration::from_secs(3)).await;
    ///
    ///         42
    ///     }).expect("undefined behavior happened"); // 42
    ///
    ///     println!("Hello from a sync runtime after at least 3 seconds with result: {}", res);
    /// }
    /// ```
    pub fn run_and_block_on_local<T, Fut: Future<Output = T>>(
        &'static mut self,
        future: Fut,
    ) -> Result<T, &'static str> {
        generate_run_and_block_on_function!(Executor::spawn_local, future, self)
    }

    /// Runs the executor with a global task and blocks on it. The executor will be stopped
    /// after the task completes.
    ///
    /// # The difference between global and local tasks
    ///
    /// Read it in [`Executor`].
    ///
    /// # Returns
    ///
    /// It returns `Err(&'static msg)` if undefined behavior happened or `Ok(T)` if everything is ok.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::{Executor, stop_executor, sleep, Local};
    /// use std::time::Duration;
    ///
    /// fn main() {
    ///     let mut executor = Executor::init();
    ///     let id = executor.id();
    ///
    ///     let res = executor.run_and_block_on_global(async move {
    ///         println!("Hello from an async runtime!");
    ///         sleep(Duration::from_secs(3)).await;
    ///
    ///         42
    ///     }).expect("undefined behavior happened"); // 42
    ///
    ///     println!("Hello from a sync runtime after at least 3 seconds with result: {}", res);
    /// }
    /// ```
    pub fn run_and_block_on_global<T, Fut: Future<Output = T> + Send>(
        &'static mut self,
        future: Fut,
    ) -> Result<T, &'static str> {
        generate_run_and_block_on_function!(Executor::spawn_global, future, self)
    }
}

// endregion

#[cfg(test)]
mod tests {
    use crate::local::Local;
    use crate::utils::global_test_lock::GLOBAL_TEST_LOCK;
    use crate::yield_now::yield_now;
    use std::ops::Deref;

    use super::*;

    #[orengine_macros::test]
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

        assert_eq!(&vec![10, 30, 20], arr.deref()); // 30, 20 because of LIFO

        let arr = Local::new(Vec::new());

        insert(10, arr.clone()).await;
        local_executor().exec_local_future(insert(20, arr.clone()));
        local_executor().exec_local_future(insert(30, arr.clone()));

        assert_eq!(&vec![10, 20, 30], arr.deref()); // 20, 30 because we don't use the list here
    }

    #[test]
    fn test_run_and_block_on() {
        let lock = GLOBAL_TEST_LOCK.lock();
        async fn async_42() -> u32 {
            42
        }

        Executor::init();
        assert_eq!(Ok(42), local_executor().run_and_block_on_local(async_42()));
        drop(lock);
    }

    // #[test]
    // TODO
    // fn work_sharing() {
    // }
}
