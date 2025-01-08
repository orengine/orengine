use crate::runtime::{local_executor, Config};
use crate::{utils, Executor};
use std::future::Future;

/// It does next steps on each core:
///
/// 1 - Initializes the [`Executor`] with provided [`Config`];
///
/// 2 - [`Spawns a local future`](Executor::spawn_local) using `creator`;
///
/// 3 - [`Runs`](Executor::run) the [`Executor`].
///
/// # State
///
/// It creates a new [`std::thread::Scope`] and runs provided `creator` in it.
/// So, it keeps the state in each core.
///
/// # Example
///
/// ## High-performance echo server
///
/// ```no_run
/// use orengine::{run_local_future_on_all_cores_with_config, local_executor};
/// use orengine::runtime::Config;
/// use orengine::io::{full_buffer, AsyncBind, AsyncAccept};
/// use orengine::net::{Stream, TcpListener, TcpStream};
///
/// use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
///
/// async fn handle_stream<S: Stream>(mut stream: S) {
///     loop {
///         stream.poll_recv().await.unwrap();
///         let mut buf = full_buffer();
///
///         let n = stream.recv(&mut buf).await.unwrap();
///         if n == 0 {
///             break;
///         }
///
///         stream.send_all(&buf.slice(..n)).await.unwrap();
///     }
/// }
///
/// fn main() {
///     let example_state = AtomicUsize::new(0); // Because of `local` it can be any `Send` type
///     // except `shared` synchronization primitives from `orengine::sync`.
///
///     run_local_future_on_all_cores_with_config(|| async {
///         let number = example_state.fetch_add(1, SeqCst) + 1;
///         println!("{number} listener started on {} core.", local_executor().core_id().id);
///
///         let mut listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
///         loop {
///             let (stream, _) = listener.accept().await.unwrap();
///             local_executor().spawn_local(async move {
///                 handle_stream(stream).await;
///             });
///         }
///     }, Config::default().set_numbers_of_blocking_workers(0).disable_work_sharing());
/// }
/// ```
#[allow(clippy::missing_panics_doc, reason = "Only std::thread can panic here")]
pub fn run_local_future_on_all_cores_with_config<'scope, Fut, F>(creator: F, cfg: Config)
where
    Fut: Future<Output = ()> + 'scope,
    F: 'scope + Clone + Send + Fn() -> Fut,
{
    let mut cores = utils::core::get_core_ids().unwrap();
    std::thread::scope(|scope| {
        for core in cores.drain(1..) {
            let creator = creator.clone();
            scope.spawn(move || {
                Executor::init_on_core_with_config(core, cfg);
                local_executor().spawn_local(creator());
                local_executor().run();
            });
        }

        Executor::init_on_core(cores[0]);
        local_executor().spawn_local(creator());
        local_executor().run();
    });
}

/// It does next steps on each core:
///
/// 1 - Initializes the [`Executor`] with provided [`Config`];
///
/// 2 - [`Spawns a local future`](Executor::spawn_local) using `creator`;
///
/// 3 - [`Runs`](Executor::run) the [`Executor`].
///
/// # State
///
/// It creates a new [`std::thread::Scope`] and runs provided `creator` in it.
/// So, it keeps the state in each core.
///
/// # Example
///
/// Read an example of `high-performance echo server` in [`run_local_future_on_all_cores_with_config`].
pub fn run_local_future_on_all_cores<'scope, Fut, F>(creator: F)
where
    Fut: Future<Output = ()> + 'scope,
    F: 'scope + Clone + Send + Fn() -> Fut,
{
    run_local_future_on_all_cores_with_config(creator, Config::default());
}

/// It does next steps on each core:
///
/// 1 - Initializes the [`Executor`] with provided [`Config`];
///
/// 2 - [`Spawns a shared future`](Executor::spawn_shared) using `creator`;
///
/// 3 - [`Runs`](Executor::run) the [`Executor`].
///
/// # State
///
/// It creates a new [`std::thread::Scope`] and runs provided `creator` in it.
/// So, it keeps the state in each core.
///
/// # Example
///
/// ## High-performance echo server
///
/// ```no_run
/// use orengine::{run_shared_future_on_all_cores_with_config, local_executor};
/// use orengine::runtime::Config;
/// use orengine::io::{full_buffer, AsyncBind, AsyncAccept};
/// use orengine::net::{Stream, TcpListener, TcpStream};
/// use orengine::sync::{WaitGroup, AsyncWaitGroup};
/// use orengine::utils::get_core_ids;
///
/// async fn handle_stream<S: Stream>(mut stream: S) {
///     loop {
///         stream.poll_recv().await.unwrap();
///         let mut buf = full_buffer();
///
///         let n = stream.recv(&mut buf).await.unwrap();
///         if n == 0 {
///             break;
///         }
///
///         stream.send_all(&buf.slice(..n)).await.unwrap();
///     }
/// }
///
/// fn main() {
///     let example_state = WaitGroup::new(); // Because of `shared` it can be any `Send` type.
///     let number_of_cores = get_core_ids().unwrap().len();
///
///     example_state.add(number_of_cores);
///
///     run_shared_future_on_all_cores_with_config(|| async {
///         // This future can be shared between executors.
///         // Shared architecture is used in this example, but in this case
///         // `Shared-nothing` architecture is better.
///
///         let has_current_executor_run_last = example_state.done() == 0;
///         example_state.wait().await;
///
///         if has_current_executor_run_last {
///             println!("All {number_of_cores} listeners started successfully");
///         }
///
///         let mut listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
///         loop {
///             let (stream, _) = listener.accept().await.unwrap();
///             local_executor().spawn_local(async move {
///                 handle_stream(stream).await;
///             });
///         }
///     }, Config::default().set_numbers_of_blocking_workers(0));
/// }
/// ```
#[allow(clippy::missing_panics_doc, reason = "Only std::thread can panic here")]
pub fn run_shared_future_on_all_cores_with_config<'scope, Fut, F>(creator: F, cfg: Config)
where
    Fut: Future<Output = ()> + Send + 'scope,
    F: 'scope + Clone + Send + Fn() -> Fut,
{
    let mut cores = utils::core::get_core_ids().unwrap();
    std::thread::scope(|scope| {
        for core in cores.drain(1..) {
            let creator = creator.clone();
            scope.spawn(move || {
                Executor::init_on_core_with_config(core, cfg);
                local_executor().spawn_shared(creator());
                local_executor().run();
            });
        }

        Executor::init_on_core(cores[0]);
        local_executor().spawn_shared(creator());
        local_executor().run();
    });
}

/// It does next steps on each core:
///
/// 1 - Initializes the [`Executor`] with provided [`Config`];
///
/// 2 - [`Spawns a shared future`](Executor::spawn_shared) using `creator`;
///
/// 3 - [`Runs`](Executor::run) the [`Executor`].
///
/// # State
///
/// It creates a new [`std::thread::Scope`] and runs provided `creator` in it.
/// So, it keeps the state in each core.
///
/// # Example
///
/// Read an example in [`run_shared_future_on_all_cores_with_config`].
pub fn run_shared_future_on_all_cores<'scope, Fut, F>(creator: F)
where
    Fut: Future<Output = ()> + Send + 'scope,
    F: 'scope + Clone + Send + Fn() -> Fut,
{
    run_shared_future_on_all_cores_with_config(creator, Config::default());
}
