use crate::bug_message::BUG_MESSAGE;
use crate::io::sys::WorkerSys;
use crate::io::worker::{get_local_worker_ref, init_local_worker, IoWorker};
use crate::io::{init_local_buf_pool, uninit_local_buf_pool};
use crate::runtime::call::Call;
use crate::runtime::config::{Config, ValidConfig};
use crate::runtime::executor::end_local_thread_and_write_into_ptr::EndLocalThreadAndWriteIntoPtr;
use crate::runtime::global_state::{register_local_executor, SubscribedState};
#[cfg(not(feature = "disable_send_task_to"))]
use crate::runtime::interaction_between_executors::{Interactor, SendTaskResult};
use crate::runtime::local_thread_pool::LocalThreadWorkerPool;
use crate::runtime::task::{Task, TaskPool};
use crate::runtime::waker::create_waker;
use crate::runtime::{get_core_id_for_executor, ExecutorSharedTaskList, Locality};
use crate::utils::{assert_hint, CoreId, ProgressiveTimeout};
use fastrand::Rng;
use std::cell::UnsafeCell;
use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use std::{mem, thread};

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
    pub(crate) static LOCAL_EXECUTOR: UnsafeCell<Option<Executor>> = const {
        UnsafeCell::new(None)
    };
}

/// Returns the thread-local executor wrapped in an [`Option`].
///
/// It is `None` if the executor is not initialized.
pub(crate) fn get_local_executor_ref() -> &'static mut Option<Executor> {
    LOCAL_EXECUTOR.with(|local_executor| unsafe { &mut *local_executor.get() })
}

/// Message that prints out when local executor is not initialized
/// but [`local_executor()`](local_executor) is called.
#[cfg(debug_assertions)]
pub const MSG_LOCAL_EXECUTOR_IS_NOT_INIT: &str = "\
------------------------------------------------------------------------------------------
|    Local executor is not initialized.                                                  |
|    Please initialize it first.                                                         |
|                                                                                        |
|    First way:                                                                          |
|    1 - let executor = Executor::init();                                                |
|    2 - executor.run_with_shared_future(your_future) or                                 |
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
|        executor.spawn_shared(your_future)                                              |
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
|        executor.run_and_block_on_shared(your_future)                                   |
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
#[inline]
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

/// The executor that runs futures in the current thread.
///
/// # The difference between `local` and `shared` task and futures
///
/// - `local` tasks and futures are executed only in the current thread.
///    It means that the tasks and futures can't be moved between threads.
///    It allows to use `Shared-nothing architecture` that means that in these futures and tasks
///    you can use [`Local`](crate::Local) and `local primitives of synchronization`.
///    Using `local` types can improve performance.
///
/// - `shared` tasks and futures can be moved between threads.
///   It allows to use `shared primitives of synchronization` and to `share work`.
///
/// # Share work
///
/// When a number of shared tasks in the executor is become greater
/// than [`runtime::Config.work_sharing_level`](Config::set_work_sharing_level) `Executor`
/// shares the half of work with other executors.
///
/// When `Executor` has no work, it tries to take tasks from other executors.
#[repr(C)]
pub struct Executor {
    core_id: CoreId,
    id: usize,
    config: ValidConfig,
    subscribed_state: Arc<SubscribedState>,
    task_pool: TaskPool,
    rng: Rng,
    progressive_timeout: ProgressiveTimeout<64, 131_072>,

    exec_series: usize,
    current_call: Call,
    start_round_time: Instant,
    /// `start_round_time` + 100 microseconds
    #[cfg(target_os = "linux")]
    start_round_time_for_deadlines: Instant,

    local_tasks: VecDeque<Task>,
    shared_tasks: VecDeque<Task>,
    shared_tasks_list: Option<Arc<ExecutorSharedTaskList>>,
    #[cfg(not(feature = "disable_send_task_to"))]
    interactor: Interactor,

    local_worker: &'static mut Option<WorkerSys>,
    thread_pool: LocalThreadWorkerPool,

    local_sleeping_tasks: BTreeMap<Instant, Task>,
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
    /// # Example
    ///
    /// ```no_run
    /// use orengine::runtime::Config;
    /// use orengine::Executor;
    /// use orengine::utils::get_core_ids;
    ///
    /// let cores = get_core_ids().unwrap();
    /// let ex = Executor::init_on_core_with_config(cores[0], Config::default());
    ///
    /// ex.spawn_local(async {
    ///     println!("Hello, world!");
    /// });
    /// ex.run()
    /// ```
    pub fn init_on_core_with_config(core_id: CoreId, config: Config) -> &'static mut Self {
        if get_local_executor_ref().is_some() {
            println!(
                "There is already an initialized executor in the current thread!\
             Not re-initializing."
            );
            return local_executor();
        }

        let valid_config = config.validate();
        crate::utils::core::set_for_current(core_id);
        let executor_id = FREE_EXECUTOR_ID.fetch_add(1, Ordering::Relaxed);
        let (shared_tasks, shared_tasks_list_cap) = if valid_config.is_work_sharing_enabled() {
            (
                Some(Arc::new(ExecutorSharedTaskList::new(executor_id))),
                MAX_NUMBER_OF_TASKS_TAKEN,
            )
        } else {
            (None, 0)
        };
        let number_of_thread_workers = valid_config.number_of_thread_workers;

        unsafe {
            if let Some(io_config) = valid_config.io_worker_config {
                init_local_worker(io_config);
                init_local_buf_pool(io_config.number_of_fixed_buffers, config.buffer_cap());
            } else {
                init_local_buf_pool(0, config.buffer_cap());
            }

            *get_local_executor_ref() = Some(Self {
                core_id,
                id: executor_id,
                config: valid_config,
                current_call: Call::default(),
                task_pool: TaskPool::default(),
                subscribed_state: Arc::new(SubscribedState::new()),
                rng: Rng::new(),
                progressive_timeout: ProgressiveTimeout::new(),

                exec_series: 0,
                start_round_time: Instant::now(),
                #[cfg(target_os = "linux")]
                start_round_time_for_deadlines: Instant::now() + Duration::from_micros(100),

                local_tasks: VecDeque::new(),
                shared_tasks: VecDeque::with_capacity(shared_tasks_list_cap),
                shared_tasks_list: shared_tasks,

                #[cfg(not(feature = "disable_send_task_to"))]
                interactor: Interactor::new(),
                local_worker: get_local_worker_ref(),
                thread_pool: LocalThreadWorkerPool::new(number_of_thread_workers),
                local_sleeping_tasks: BTreeMap::new(),
            });

            local_executor()
        }
    }

    /// Initializes the executor in the current thread on the given core.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::Executor;
    /// use orengine::utils::get_core_ids;
    ///
    /// let cores = get_core_ids().unwrap();
    /// let ex = Executor::init_on_core(cores[0]);
    ///
    /// ex.spawn_local(async {
    ///     println!("Hello, world!");
    /// });
    /// ex.run();
    /// ```
    pub fn init_on_core(core_id: CoreId) -> &'static mut Self {
        Self::init_on_core_with_config(core_id, Config::default())
    }

    /// Initializes the executor in the current thread with provided config.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::runtime::Config;
    /// use orengine::Executor;
    ///
    /// let ex = Executor::init_with_config(Config::default());
    ///
    /// ex.spawn_local(async {
    ///     println!("Hello, world!");
    /// });
    /// ex.run();
    /// ```
    pub fn init_with_config(config: Config) -> &'static mut Self {
        Self::init_on_core_with_config(get_core_id_for_executor(), config)
    }

    /// Initializes the executor in the current thread.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::Executor;
    ///
    /// let ex = Executor::init();
    ///
    /// ex.spawn_local(async {
    ///     println!("Hello, world!");
    /// });
    /// ex.run();
    /// ```
    pub fn init() -> &'static mut Self {
        Self::init_on_core(get_core_id_for_executor())
    }

    /// Returns the id of the executor.
    pub fn id(&self) -> usize {
        self.id
    }

    /// Returns a reference to the [`TaskPool`] of the executor.
    pub(crate) fn task_pool(&mut self) -> &mut TaskPool {
        &mut self.task_pool
    }

    /// Add a task to the beginning of the local lifo queue.
    #[inline]
    pub(crate) fn add_task_at_the_start_of_lifo_local_queue(&mut self, task: Task) {
        debug_assert!(task.is_local());

        self.local_tasks.push_front(task);
    }

    /// Add a task to the beginning of the shared lifo queue.
    #[inline]
    pub(crate) fn add_task_at_the_start_of_lifo_shared_queue(&mut self, task: Task) {
        debug_assert!(!task.is_local());

        self.shared_tasks.push_front(task);
    }

    /// Returns a reference to the subscribed state of the executor.
    pub(crate) fn subscribed_state(&self) -> Arc<SubscribedState> {
        self.subscribed_state.clone()
    }

    /// Returns a reference to the [`Interactor`] of the executor.
    #[cfg(not(feature = "disable_send_task_to"))]
    pub(crate) fn interactor(&self) -> &Interactor {
        &self.interactor
    }

    /// Returns the core id on which the executor is running.
    pub fn core_id(&self) -> CoreId {
        self.core_id
    }

    /// Returns the [`config`](Config) of the executor.
    pub fn config(&self) -> Config {
        Config::from(&self.config)
    }

    /// Returns when current round started.
    pub fn start_round_time(&self) -> Instant {
        self.start_round_time
    }

    /// Returns the approximate current time.
    ///
    /// It is obtained by adding 100 microseconds to the
    /// [`start time of the current round`](Self::start_round_time).
    ///
    /// This method is supposed to be used to set deadlines,
    /// therefore returning a future time is acceptable.
    ///
    /// # Behavior on fallback OS
    ///
    /// In fallback, we can't guarantee the 100 microseconds addition sufficiency,
    /// therefore it is a synonymous to [`Instant::now`] there.
    pub fn start_round_time_for_deadlines(&self) -> Instant {
        #[cfg(target_os = "linux")]
        {
            self.start_round_time_for_deadlines
        }

        #[cfg(not(target_os = "linux"))]
        {
            Instant::now()
        }
    }

    /// Returns a reference to the shared tasks list of the executor.
    pub(crate) fn shared_task_list(&self) -> Option<&Arc<ExecutorSharedTaskList>> {
        self.shared_tasks_list.as_ref()
    }

    /// Returns a reference to the local tasks queue.
    #[inline]
    pub fn local_queue(&mut self) -> &mut VecDeque<Task> {
        &mut self.local_tasks
    }

    /// Returns a reference to the `sleeping_tasks`.
    #[inline]
    pub(crate) fn sleeping_tasks(&mut self) -> &mut BTreeMap<Instant, Task> {
        &mut self.local_sleeping_tasks
    }

    /// Returns the number of spawned tasks (shared and local).
    pub(crate) fn number_of_spawned_tasks(&self) -> usize {
        self.shared_tasks.len() + self.local_tasks.len()
    }

    /// Invokes the current [`Call`].
    ///
    /// # Safety
    ///
    /// This function is unsafe because it can break the program if provided [`Call`] is used
    /// with wrong arguments. Think twice before using [`Calls`](Call) and twice read the `Safety`
    /// region of provided [`Call`] before using it.
    #[inline]
    pub unsafe fn invoke_call(&mut self, call: Call) {
        debug_assert!(self.current_call.is_none());

        self.current_call = call;
    }

    /// Processing current [`Call`]. It is taken out [`exec_task_now`](Executor::exec_task_now)
    /// to allow the compiler to decide whether to inline this function.
    #[inline(never)]
    fn handle_call(&mut self, mut task: Task) {
        match mem::take(&mut self.current_call) {
            Call::None => {}
            Call::PushCurrentTaskAtTheStartOfLIFOSharedQueue => {
                self.shared_tasks.push_front(task);
            }
            Call::PushCurrentTaskTo(task_list) => unsafe { (*task_list).push(task) },
            Call::PushCurrentTaskToAndRemoveItIfCounterIsZero(task_list, counter, order) => {
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
            Call::ChangeCurrentTaskLocality(locality) => {
                task.data.set_locality(locality);
                assert_eq!(
                    task.is_local(),
                    locality.is_local(),
                    "locality is {}, local is {}, shared is {}",
                    locality.value,
                    Locality::local().value,
                    Locality::shared().value
                );

                if locality.is_local() {
                    self.spawn_local_task(task);
                } else {
                    self.spawn_shared_task(task);
                }
            }
        }
    }

    /// Executes a provided [`task`](Task) in the current [`executor`](Executor).
    ///
    /// # The difference between `exec_task_now` and [`exec_task`](Executor::exec_task)
    ///
    /// If the stack of calls is too large, [`exec_task`](Executor::exec_task)
    /// spawns a new task and returns.
    /// Otherwise, [`exec_task_now`](Executor::exec_task_now) executes the task anyway.
    ///
    /// # Attention
    ///
    /// Execute [`tasks`](Task) only by this method or [`exec_task`](Executor::exec_task)!
    pub fn exec_task_now(&mut self, mut task: Task) {
        self.exec_series += 1;

        let future = unsafe { &mut *task.future_ptr() };
        #[cfg(debug_assertions)]
        unsafe {
            task.check_safety();
            task.is_executing.as_ref().store(true, Ordering::SeqCst);
        }

        let waker = create_waker(&mut task);
        let mut context = Context::from_waker(&waker);
        let poll_res = unsafe { Pin::new_unchecked(future) }
            .as_mut()
            .poll(&mut context);
        #[cfg(debug_assertions)]
        unsafe {
            task.is_executing.as_ref().store(false, Ordering::SeqCst);
        }

        match poll_res {
            Poll::Ready(()) => {
                debug_assert_eq!(
                    self.current_call,
                    Call::None,
                    "Call is not None, but the task is ready."
                );
                unsafe { task.release(self) };
            }

            Poll::Pending => {
                if !matches!(self.current_call, Call::None) {
                    self.handle_call(task);
                }
            }
        }

        // orengine::Waker::drop does nothing, but virtual call is not free.
        mem::forget(waker);
    }

    /// Executes a provided [`task`](Task) in the current [`executor`](Executor).
    ///
    /// # Attention
    ///
    /// Execute [`tasks`](Task) only by this method or [`exec_task_now`](Executor::exec_task_now)!
    #[inline]
    pub fn exec_task(&mut self, task: Task) {
        if self.exec_series < 63 {
            self.exec_task_now(task);

            return;
        }

        self.exec_series = 0;
        if task.is_local() {
            self.spawn_local_task(task);
        } else {
            self.spawn_shared_task(task);
        }
    }

    /// Creates a `local` [`task`](Task) from a provided [`future`](Future)
    /// and executes it in the current [`executor`](Executor).
    ///
    /// # Attention
    ///
    /// Execute [`Future`] only by this method!
    #[inline]
    pub fn exec_local_future<F>(&mut self, future: F)
    where
        F: Future<Output = ()>,
    {
        let task = unsafe { Task::from_future(future, Locality::local()) };
        self.exec_task(task);
    }

    /// Creates a `shared` [`task`](Task) from a provided [`future`](Future)
    /// and executes it in the current [`executor`](Executor).
    ///
    /// # Attention
    ///
    /// Execute [`Future`] only by this method!
    #[inline]
    pub fn exec_shared_future<F>(&mut self, future: F)
    where
        F: Future<Output = ()> + Send,
    {
        let task = unsafe { Task::from_future(future, Locality::shared()) };
        self.exec_task(task);
    }

    /// Creates a `local` [`task`](Task) from a provided [`future`](Future) and enqueues it.
    ///
    /// # Attention
    ///
    /// This function enqueues it at the end of the queue of local tasks, but it is `LIFO`.
    ///
    /// # The difference between shared and local tasks
    ///
    /// Read it in [`Executor`].
    #[inline]
    pub fn spawn_local<F>(&mut self, future: F)
    where
        F: Future<Output = ()>,
    {
        let task = unsafe { Task::from_future(future, Locality::local()) };
        self.spawn_local_task(task);
    }

    /// Enqueues a `local` [`task`](Task).
    ///
    /// # Attention
    ///
    /// This function enqueues it at the end of the queue of local tasks, but it is `LIFO`.
    ///
    /// # The difference between shared and local tasks
    ///
    /// Read it in [`Executor`].
    #[inline]
    pub fn spawn_local_task(&mut self, task: Task) {
        debug_assert!(task.is_local(), "Try to spawn `shared` task as `local`!");

        self.local_tasks.push_back(task);
    }

    /// Creates a `shared` [`task`](Task) from a provided [`future`](Future) and enqueues it.
    ///
    /// # Attention
    ///
    /// This function enqueues it at the end of the queue of shared tasks, but it is `LIFO`.
    ///
    /// # The difference between shared and local tasks
    ///
    /// Read it in [`Executor`].
    #[inline]
    pub fn spawn_shared<F>(&mut self, future: F)
    where
        F: Future<Output = ()> + Send,
    {
        let task = unsafe { Task::from_future(future, Locality::shared()) };
        self.spawn_shared_task(task);
    }

    /// Enqueues a `shared` [`task`](Task).
    ///
    /// # Attention
    ///
    /// This function enqueues it at the end of the queue of shared tasks, but it is `LIFO`.
    ///
    /// # The difference between shared and local tasks
    ///
    /// Read it in [`Executor`].
    #[inline]
    pub fn spawn_shared_task(&mut self, task: Task) {
        fn try_flush(executor: &mut Executor) {
            if let Some(mut shared_tasks_list) = unsafe {
                executor
                    .shared_tasks_list
                    .as_ref()
                    .unwrap_unchecked()
                    .try_lock_and_return_as_vec()
            } {
                let number_of_shared = (executor.shared_tasks.len() >> 1) + 1;
                for task in executor.shared_tasks.drain(..number_of_shared) {
                    shared_tasks_list.push(task);
                }
            }
        }

        debug_assert!(!task.is_local(), "Try to spawn `local` task as `shared`!");

        #[allow(clippy::branches_sharing_code, reason = "It is more readable")]
        if self.config.is_work_sharing_enabled() {
            if self.shared_tasks.len() <= self.config.work_sharing_level {
                // Fast path
                self.shared_tasks.push_back(task);
            } else {
                // Slow path
                try_flush(self);

                self.shared_tasks.push_back(task);
            }
        } else {
            self.shared_tasks.push_back(task);
        }
    }

    /// Calls [`spawn_local_task`](Executor::spawn_local_task)
    /// or [`spawn_shared_task`](Executor::spawn_shared_task) depending on
    /// the [`locality`](Locality) of the provided [`task`](Task).
    ///
    /// It is a little bit slower than calling them directly.
    #[inline]
    pub fn spawn_task(&mut self, task: Task) {
        if task.is_local() {
            self.spawn_local_task(task);
        } else {
            self.spawn_shared_task(task);
        }
    }

    /// Sends a [`Task`] to the executor with the given id.
    ///
    /// It is unsafe because we can't check if the provided [`Task`] don't reference to the
    /// current thread non-Send data.
    ///
    /// If provided `executor_id` is equal to the current executor id, this function calls
    /// [`spawn_task`](Executor::spawn_task) instead.
    ///
    /// # Safety
    ///
    /// - Provided [`Task`] must not reference to the current thread non-Send data;
    ///
    /// - If it is `shared`, it valid when the [`Task`] is valid;
    ///
    /// - If it is `local`, it valid only when the [`Task`] doesn't reference to the
    ///   current thread non-Send data. For example, it can contain [`Local`](crate::Local)
    ///   created in the current thread, or it can reference to some non-Send data
    ///   if these data are used only in the thread where provided [`Task`] will be sent.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::collections::HashMap;
    /// use orengine::net::{TcpListener, Stream};
    /// use orengine::local_executor;
    /// use orengine::runtime::{Task, Locality};
    /// use orengine::io::{full_buffer, AsyncBind, AsyncAccept};
    /// use orengine::run_local_future_on_all_cores;
    ///
    /// thread_local! {
    ///     static STORAGE_SHARD: HashMap<u32, usize> = HashMap::new();
    /// }
    ///
    /// fn get_value_from_local_shard(key: u32) -> Option<usize> {
    ///     STORAGE_SHARD.with(|shard| shard.get(&key).cloned())
    /// }
    ///
    /// async fn handle_connection<S: Stream>(mut stream: S) {
    ///     stream.poll_recv().await.unwrap();
    ///
    ///     let mut buffer = full_buffer();
    ///
    ///     stream.recv_exact(&mut buffer.slice_mut(0..8)).await.unwrap();
    ///     let number_of_shard = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
    ///
    ///     let key = u32::from_be_bytes([buffer[4], buffer[5], buffer[6], buffer[7]]);
    ///
    ///     drop(buffer); // release buffer in this thread
    ///
    ///     let task = unsafe {
    ///         Task::from_future(async move {
    ///             let value = get_value_from_local_shard(key).unwrap();
    ///             let mut buffer = orengine::io::buffer();
    ///
    ///             buffer.append(&value.to_be_bytes());
    ///             stream.send_all(&buffer).await.unwrap();
    ///         }, Locality::local())
    ///     };
    ///
    ///     unsafe {
    ///         local_executor()
    ///             .send_task_to_executor(task, number_of_shard)
    ///             .expect("Executor with such id doesn't exist");
    ///     }
    /// }
    ///
    /// fn main() {
    ///     run_local_future_on_all_cores(|| async {
    ///         let mut listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
    ///         let executor = local_executor();
    ///
    ///         loop {
    ///             let (stream, _) = listener.accept().await.unwrap();
    ///             executor.spawn_local(async move {
    ///                 handle_connection(stream).await
    ///             });
    ///         }
    ///     });
    /// }
    /// ```
    #[cfg(not(feature = "disable_send_task_to"))]
    pub unsafe fn send_task_to_executor(
        &mut self,
        task: Task,
        executor_id: usize,
    ) -> SendTaskResult {
        if executor_id != self.id {
            self.interactor.send_task_to_executor(task, executor_id)
        } else {
            self.spawn_task(task);

            SendTaskResult::Ok
        }
    }

    /// Creates a new `local` [`Task`] and sends it to the executor with the given id.
    ///
    /// If provided `executor_id` is equal to the current executor id, this function calls
    /// [`spawn_task`](Executor::spawn_task) instead.
    ///
    /// # Example
    ///
    /// Very detailed example can be found in [`send_task_to_executor`](Self::send_task_to_executor).
    ///
    /// ```no_run
    /// use orengine::io::AsyncSend;
    /// use orengine::local_executor;
    /// use orengine::net::Stream;
    /// # fn get_local_storage() -> &'static mut std::collections::HashMap<usize, usize> {
    /// #     Box::leak(Box::new(std::collections::HashMap::new()))
    /// # }
    /// # fn get_shard_id(request: &Request<impl AsyncSend>) -> usize { 0 }
    /// # async fn read_request<S: Stream>(mut stream: S) -> Request<S> { Request { shard_id: 0, key: 0, response_sender: stream } }
    ///
    /// struct Request<Sender: AsyncSend> {
    ///     shard_id: usize,
    ///     key: usize,
    ///     response_sender: Sender
    /// }
    ///
    /// async fn process_stream<S: Stream>(mut stream: S) {
    ///     let mut request = read_request(stream).await;
    ///     let shard_id = get_shard_id(&request);
    ///
    ///     local_executor()
    ///         .send_local_future_to_executor(|| async {
    ///             let mut storage = get_local_storage();
    ///             let value = storage.get(&request.key).unwrap();
    ///
    ///             request.response_sender.send_all_bytes(&value.to_be_bytes()).await.unwrap();
    ///         }, shard_id)
    ///         .expect("Executor with such id doesn't exist");
    /// }
    /// ```
    #[cfg(not(feature = "disable_send_task_to"))]
    pub fn send_local_future_to_executor<Fut, F>(
        &mut self,
        creator: F,
        executor_id: usize,
    ) -> SendTaskResult
    where
        Fut: Future<Output = ()>,
        F: FnOnce() -> Fut,
    {
        let task = unsafe { Task::from_future(creator(), Locality::local()) };

        unsafe { self.send_task_to_executor(task, executor_id) }
    }

    /// Creates a new `local` [`Task`] and sends it to the executor with the given id.
    ///
    /// If provided `executor_id` is equal to the current executor id, this function calls
    /// [`spawn_task`](Executor::spawn_task) instead.
    ///
    /// # Example
    ///
    /// Very detailed example can be found in [`send_task_to_executor`](Self::send_task_to_executor).
    ///
    /// And more simple example can be found in
    /// [`send_local_future_to_executor`](Self::send_local_future_to_executor).
    #[cfg(not(feature = "disable_send_task_to"))]
    pub fn send_shared_future_to_executor<Fut, F>(
        &mut self,
        creator: F,
        executor_id: usize,
    ) -> SendTaskResult
    where
        Fut: Future<Output = ()> + Send,
        F: FnOnce() -> Fut,
    {
        let task = unsafe { Task::from_future(creator(), Locality::shared()) };

        unsafe { self.send_task_to_executor(task, executor_id) }
    }

    /// Tries to take a batch of tasks from the shared tasks queue if needed.
    #[inline]
    fn take_work_if_needed(&mut self) {
        if self.shared_tasks.len() >= self.config.work_sharing_level {
            return;
        }

        if let Some(shared_task_list) = self.shared_tasks_list.as_mut() {
            if let Some(mut shared_task_list) = shared_task_list.try_lock_and_return_as_vec() {
                if !shared_task_list.is_empty() {
                    let limit = self.config.work_sharing_level - self.shared_tasks.len(); // Always bigger than 0, because of previous checks
                    for _ in 0..limit {
                        if let Some(task) = shared_task_list.pop() {
                            self.shared_tasks.push_back(task);
                        }
                    }

                    shrink!(shared_task_list);

                    return;
                }
            }

            unsafe {
                self.subscribed_state.with_tasks_lists(|lists| {
                    if lists.is_empty() {
                        return;
                    }

                    let max_number_of_tries = self.rng.usize(0..lists.len()) + 1;

                    for i in 0..max_number_of_tries {
                        let list = lists.get(i).expect(BUG_MESSAGE);
                        let limit = MAX_NUMBER_OF_TASKS_TAKEN - self.shared_tasks.len();
                        if limit == 0 {
                            return;
                        }

                        list.take_batch(&mut self.shared_tasks, limit);
                    }
                });
            }
        }
    }

    /// Allows the OS to run other threads.
    ///
    /// It is used only when no work is available.
    #[inline]
    #[allow(clippy::unused_self, reason = "It will be used in the future")]
    fn sleep_at_most(&self, max_duration: Duration) {
        // Wait for more work

        thread::sleep(max_duration);
    }

    /// Wakes up sleeping tasks that are ready to be executed.
    ///
    /// Returns the duration for the nearest deadline of the sleeping tasks or None
    /// if there are no sleeping tasks.
    #[inline]
    fn check_sleeping_tasks(&mut self) -> Option<Duration> {
        if !self.local_sleeping_tasks.is_empty() {
            self.start_round_time = Instant::now();

            while let Some((time_to_wake, task)) = self.local_sleeping_tasks.pop_first() {
                if time_to_wake <= self.start_round_time {
                    if task.is_local() {
                        self.exec_task(task);
                    } else {
                        self.spawn_shared_task(task);
                    }
                } else {
                    self.local_sleeping_tasks.insert(time_to_wake, task);
                    return Some(self.start_round_time - time_to_wake);
                }
            }
        }

        None
    }

    /// Prepares the executor for the next round.
    fn prepare_to_new_round(&mut self) {
        self.exec_series = 0;
        self.start_round_time = Instant::now();
        #[cfg(target_os = "linux")]
        {
            self.start_round_time_for_deadlines =
                self.start_round_time + Duration::from_micros(100);
        }
    }

    /// Executes all ready CPU tasks.
    #[inline]
    fn exec_cpu_tasks(&mut self) {
        // A round is a number of tasks that must be completed before the next background_work call.
        // It is needed to avoid case like:
        //   Task with yield -> repeat this task -> repeat this task -> ...
        //
        // So it works like:
        //   Round 1 -> background work -> round 2  -> ...

        let mut task;

        let number_of_local_tasks_in_this_round = self.local_tasks.len();
        for _ in 0..number_of_local_tasks_in_this_round {
            assert_hint(
                !self.local_tasks.is_empty(),
                "number_of_local_tasks_in_this_round is invalid",
            );

            task = unsafe { self.local_tasks.pop_back().unwrap_unchecked() };
            self.exec_task(task);
        }

        let number_of_shared_tasks_in_this_round = self.shared_tasks.len();
        for _ in 0..number_of_shared_tasks_in_this_round {
            if let Some(task) = self.shared_tasks.pop_back() {
                self.exec_task(task);
            } else {
                // Executor shared its tasks with another one.
                break;
            }
        }
    }

    /// Stop the executor with all necessary actions.
    ///
    /// # Safety
    ///
    /// Called after [`check_version_and_update_if_needed`](SubscribedState::check_version_and_update_if_needed).
    #[inline(never)]
    unsafe fn graceful_stop(&mut self) {
        uninit_local_buf_pool();
        if self.config.is_work_sharing_enabled() {
            unsafe {
                self.subscribed_state.with_tasks_lists(|lists| {
                    if let Some(first_neighbor) = lists.first() {
                        let mut shared_tasks_list_of_current_executor = vec![];
                        loop {
                            if let Some(mut tasks_list) = self
                                .shared_tasks_list
                                .as_ref()
                                .unwrap()
                                .try_lock_and_return_as_vec()
                            {
                                shared_tasks_list_of_current_executor.extend(tasks_list.drain(..));
                                break;
                            }
                        }

                        let mut tasks_list;
                        loop {
                            if let Some(tasks_list_) = first_neighbor.try_lock_and_return_as_vec() {
                                tasks_list = tasks_list_;
                                break;
                            }
                        }

                        tasks_list.extend(self.shared_tasks.drain(..));
                        tasks_list.extend(shared_tasks_list_of_current_executor);
                    }
                });
            }
        }

        *get_local_executor_ref() = None;
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
    /// let mut executor = Executor::init();
    /// let id = executor.id();
    ///
    /// executor.spawn_local(async move {
    ///     println!("Hello from an async runtime!");
    ///     sleep(Duration::from_secs(3)).await;
    ///     stop_executor(id); // stop the executor
    /// });
    /// executor.run();
    ///
    /// println!("Hello from a sync runtime after at least 3 seconds");
    /// ```
    pub fn run(&mut self) {
        register_local_executor();

        loop {
            self.subscribed_state.check_version_and_update_if_needed(
                self.id,
                #[cfg(not(feature = "disable_send_task_to"))]
                &mut self.interactor,
            );
            if self.subscribed_state.is_stopped() {
                break;
            }

            self.prepare_to_new_round();

            #[cfg(not(feature = "disable_send_task_to"))]
            {
                self.interactor.do_work(
                    &mut self.local_tasks,
                    &mut self.shared_tasks,
                    self.start_round_time,
                );
            }
            self.exec_cpu_tasks();
            self.take_work_if_needed();
            self.thread_pool.poll(&mut self.local_tasks);
            let nearest_timeout_option = self.check_sleeping_tasks();

            // We need to consider 8 cases from 3 variables:
            // has cpu work (self.number_of_spawned_tasks() != 0 or self.config.is_work_sharing_enabled()),
            // has io work and has sleeping tasks
            let has_cpu_work = self.number_of_spawned_tasks() > 0;
            if self.local_worker.is_some() {
                let worker = unsafe { self.local_worker.as_mut().unwrap_unchecked() };
                if worker.has_work() {
                    if !has_cpu_work {
                        let max_timeout = self.progressive_timeout.timeout_with_shift(2);
                        if let Some(nearest_timeout) = nearest_timeout_option {
                            // case 1: we don't have cpu work, but we have sleeping tasks and io work
                            worker.must_poll(Some(nearest_timeout.min(max_timeout)));
                        } else {
                            // case 2: we don't have cpu work nor sleeping tasks, but we io work
                            worker.must_poll(Some(max_timeout));
                        }
                    } else {
                        // It proceeds 2 cases:
                        // case 3: we have cpu work, sleeping tasks and io work
                        // case 4: we have cpu work, io work, but we don't have sleeping tasks
                        self.progressive_timeout.reset();
                        worker.must_poll(None);
                    }
                } else if !has_cpu_work {
                    let max_timeout = self.progressive_timeout.timeout();
                    if let Some(nearest_timeout) = nearest_timeout_option {
                        // case 5: we don't have io work nor cpu work, but we have sleeping tasks
                        self.sleep_at_most(nearest_timeout.min(max_timeout));
                    } else {
                        // case 6: we don't have any work
                        self.sleep_at_most(max_timeout);
                    }
                } else {
                    // It proceeds 2 cases:
                    // case 7: we have cpu work, sleeping tasks, but don't have io work
                    // case 8: we have cpu work, but don't have io work nor sleeping tasks

                    self.progressive_timeout.reset();

                    // Continue processing cpu tasks
                }
            } else {
                // Here we don't have worker, therefore we need to consider only 4 cases

                let max_timeout = self.progressive_timeout.timeout();

                if let Some(nearest_timeout) = nearest_timeout_option {
                    if has_cpu_work {
                        // case 1: we have cpu work and sleeping tasks
                        // Continue processing cpu tasks
                    } else {
                        // case 2: we have sleeping tasks, but we don't have cpu work
                        self.sleep_at_most(nearest_timeout.min(max_timeout));
                    }

                    self.progressive_timeout.reset();
                } else if has_cpu_work {
                    // case 3: we have cpu work, but we don't have sleeping tasks
                    // Continue processing cpu tasks

                    self.progressive_timeout.reset();
                } else {
                    // case 4: we don't have cpu work nor sleeping tasks
                    self.sleep_at_most(max_timeout);
                }
            }

            shrink!(self.local_tasks);
        }

        unsafe { self.graceful_stop() };
    }

    /// Runs the executor with a local task.
    ///
    /// # The difference between shared and local tasks
    ///
    /// Read it in [`Executor`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::{Executor, stop_executor, sleep, Local};
    /// use std::time::Duration;
    ///
    /// let mut executor = Executor::init();
    /// let id = executor.id();
    /// let local_msg = Local::new("Hello from an async runtime!"); // bad example of usage Local,
    /// // but you can use Local, because here we use a local task.
    ///
    /// executor.run_with_local_future(async move {
    ///     println!("{}" ,local_msg);
    ///     sleep(Duration::from_secs(3)).await;
    ///     stop_executor(id); // stops the executor
    /// });
    ///
    /// println!("Hello from a sync runtime after at least 3 seconds");
    /// ```
    pub fn run_with_local_future<Fut: Future<Output = ()>>(&mut self, future: Fut) {
        self.spawn_local(future);
        self.run();
    }

    /// Runs the executor with a shared task.
    ///
    /// # The difference between shared and local tasks
    ///
    /// Read it in [`Executor`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::{Executor, stop_executor, sleep, Local};
    /// use std::time::Duration;
    ///
    /// let mut executor = Executor::init();
    /// let id = executor.id();
    ///
    /// executor.run_with_shared_future(async move {
    ///     println!("Hello from an async runtime!");
    ///     sleep(Duration::from_secs(3)).await;
    ///     stop_executor(id); // stops the executor
    /// });
    ///
    /// println!("Hello from a sync runtime after at least 3 seconds");
    /// ```
    pub fn run_with_shared_future<Fut: Future<Output = ()> + Send>(&mut self, future: Fut) {
        self.spawn_shared(future);
        self.run();
    }

    /// Runs the executor with a local task and blocks on it. The executor will be stopped
    /// after the task completes.
    ///
    /// # The difference between shared and local tasks
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
    /// let mut executor = Executor::init();
    /// let id = executor.id();
    /// let local_msg = Local::new("Hello from an async runtime!"); // bad example of usage Local,
    /// // but you can use Local, because here we use a local task.
    ///
    /// let res = executor.run_and_block_on_local(async move {
    ///     println!("{}" ,local_msg);
    ///     sleep(Duration::from_secs(3)).await;
    ///
    ///     42
    /// }).expect("undefined behavior happened"); // 42
    ///
    /// println!("Hello from a sync runtime after at least 3 seconds with result: {}", res);
    /// ```
    pub fn run_and_block_on_local<T, Fut: Future<Output = T>>(
        &'static mut self,
        future: Fut,
    ) -> Result<T, &'static str> {
        generate_run_and_block_on_function!(Self::spawn_local, future, self)
    }

    /// Runs the executor with a shared task and blocks on it. The executor will be stopped
    /// after the task completes.
    ///
    /// # The difference between shared and local tasks
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
    /// let mut executor = Executor::init();
    /// let id = executor.id();
    ///
    /// let res = executor.run_and_block_on_shared(async move {
    ///     println!("Hello from an async runtime!");
    ///     sleep(Duration::from_secs(3)).await;
    ///
    ///     42
    /// }).expect("undefined behavior happened"); // 42
    ///
    /// println!("Hello from a sync runtime after at least 3 seconds with result: {}", res);
    /// ```
    pub fn run_and_block_on_shared<T, Fut: Future<Output = T> + Send>(
        &'static mut self,
        future: Fut,
    ) -> Result<T, &'static str> {
        generate_run_and_block_on_function!(Self::spawn_shared, future, self)
    }
}

// endregion

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::local::Local;
    use crate::yield_now::yield_now;

    #[orengine::test::test_local]
    fn test_spawn_local_and_exec_future() {
        #[allow(clippy::unused_async, reason = "It is a test.")]
        #[allow(clippy::future_not_send, reason = "It is a test.")]
        async fn insert(number: u16, arr: Local<Vec<u16>>) {
            arr.borrow_mut().push(number);
        }

        let executor = local_executor();
        let arr = Local::new(Vec::new());

        insert(10, arr.clone()).await;
        executor.spawn_local(insert(20, arr.clone()));
        executor.spawn_local(insert(30, arr.clone()));

        yield_now().await;

        assert_eq!(&vec![10, 30, 20], &*arr.borrow()); // 30, 20 because of LIFO

        let arr = Local::new(Vec::new());

        insert(10, arr.clone()).await;
        local_executor().exec_local_future(insert(20, arr.clone()));
        local_executor().exec_local_future(insert(30, arr.clone()));

        assert_eq!(&vec![10, 20, 30], &*arr.borrow()); // 20, 30 because we don't use the list here
    }

    #[test]
    fn test_run_and_block_on() {
        #[allow(clippy::unused_async, reason = "It is a test.")]
        async fn async_42() -> u32 {
            42
        }

        Executor::init_with_config(Config::default().disable_work_sharing());
        assert_eq!(Ok(42), local_executor().run_and_block_on_local(async_42()));
    }

    // TODO put this into a separate test
    // fn wait_for_config_tests_ready() {
    //     let _unused = config::tests::WAS_READY
    //         .1
    //         .wait_while(config::tests::WAS_READY.0.lock().unwrap(), |was_ready| {
    //             !*was_ready
    //         })
    //         .unwrap();
    // }
    //
    // fn test_with_work_sharing_level(level: usize) {
    //     const N: usize = 5;
    //
    //     wait_for_config_tests_ready();
    //
    //     Executor::init_with_config(Config::default().set_work_sharing_level(level))
    //         .run_and_block_on_shared(async {
    //             for _ in 0..10 {
    //                 let wg = Arc::new(WaitGroup::new());
    //                 let counter = Arc::new(AtomicUsize::new(0));
    //                 for _ in 0..N {
    //                     let wg = wg.clone();
    //                     let counter = counter.clone();
    //                     wg.inc();
    //                     local_executor().spawn_shared(async move {
    //                         counter.fetch_add(1, SeqCst);
    //                         wg.done();
    //                     });
    //                 }
    //
    //                 create_test_dir_if_not_exist();
    //                 let file = Arc::new(Mutex::new(
    //                     File::open(
    //                         format!("{TEST_DIR_PATH}/test_task_sharing_level={level}.txt"),
    //                         &OpenOptions::new().create(true).truncate(true).write(true),
    //                     )
    //                     .await
    //                     .unwrap(),
    //                 ));
    //
    //                 for _ in 0..N {
    //                     let wg = wg.clone();
    //                     let counter = counter.clone();
    //                     let file = file.clone();
    //                     wg.inc();
    //                     local_executor().spawn_shared(async move {
    //                         file.lock()
    //                             .await
    //                             .write_all(b"test")
    //                             .await
    //                             .expect("Can't write to file");
    //
    //                         counter.fetch_add(1, SeqCst);
    //                         wg.done();
    //                     });
    //                 }
    //
    //                 wg.wait().await;
    //                 assert_eq!(N * 2, counter.load(SeqCst));
    //             }
    //         })
    //         .expect("undefined behavior happened");
    // }
    //
    // #[test]
    // fn test_work_sharing_zero_level() {
    //     test_with_work_sharing_level(0);
    // }
    //
    // #[test]
    // fn test_work_sharing_one_level() {
    //     test_with_work_sharing_level(1);
    // }
    //
    // #[test]
    // fn test_work_sharing_min_level() {
    //     test_with_work_sharing_level(16);
    // }
    //
    // #[test]
    // fn test_work_sharing_large_level() {
    //     test_with_work_sharing_level(255);
    // }
    //
    // #[orengine::test::test_shared]
    // #[cfg(not(feature = "disable_send_task_to"))]
    // fn test_send_task() {
    //     use crate::sync::{AsyncWaitGroup, WaitGroup};
    //     use crate::test::sched_future_to_another_thread;
    //     use std::sync::atomic::Ordering::SeqCst;
    //
    //     static RESULTED_TASKS: AtomicUsize = AtomicUsize::new(0);
    //
    //     let another_id = Arc::new(std::sync::Mutex::new(usize::MAX));
    //     let another_id_clone = another_id.clone();
    //     let cond_var = Arc::new(std::sync::Condvar::new());
    //     let cond_var_clone = cond_var.clone();
    //     let wg = Arc::new(WaitGroup::new());
    //
    //     let wg_clone = wg.clone();
    //
    //     wg_clone.inc();
    //     sched_future_to_another_thread(async move {
    //         let id = local_executor().id();
    //         *another_id_clone.lock().unwrap() = id;
    //         cond_var_clone.notify_one();
    //
    //         wg_clone.wait().await;
    //     });
    //
    //     let another_executor_id = *cond_var
    //         .wait_while(another_id.lock().unwrap(), |guard| *guard == usize::MAX)
    //         .unwrap();
    //
    //     yield_now().await;
    //
    //     let task = unsafe {
    //         let wg_clone = wg.clone();
    //         wg_clone.inc();
    //
    //         Task::from_future(
    //             async move {
    //                 assert_eq!(local_executor().id(), another_executor_id);
    //
    //                 RESULTED_TASKS.fetch_add(1, SeqCst);
    //
    //                 wg_clone.done();
    //             },
    //             Locality::local(),
    //         )
    //     };
    //
    //     unsafe {
    //         local_executor()
    //             .send_task_to_executor(task, another_executor_id)
    //             .expect("Failed to send task");
    //     };
    //
    //     // send local future
    //     {
    //         let wg_clone = wg.clone();
    //         wg_clone.inc();
    //
    //         local_executor()
    //             .send_local_future_to_executor(
    //                 || async move {
    //                     assert_eq!(local_executor().id(), another_executor_id);
    //
    //                     RESULTED_TASKS.fetch_add(1, SeqCst);
    //
    //                     wg_clone.done();
    //                 },
    //                 another_executor_id,
    //             )
    //             .expect("Failed to send future");
    //     }
    //
    //     // send shared future
    //     {
    //         let wg_clone = wg.clone();
    //         wg_clone.inc();
    //
    //         local_executor()
    //             .send_shared_future_to_executor(
    //                 || async move {
    //                     assert_eq!(local_executor().id(), another_executor_id);
    //
    //                     RESULTED_TASKS.fetch_add(1, SeqCst);
    //
    //                     wg_clone.done();
    //                 },
    //                 another_executor_id,
    //             )
    //             .expect("Failed to send future");
    //     }
    //
    //     wg.done();
    //     wg.wait().await;
    //
    //     assert_eq!(RESULTED_TASKS.load(SeqCst), 3);
    // }
}
