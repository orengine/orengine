use crate::runtime::{local_executor, Config};
use crate::{utils, Executor};
use std::future::Future;

/// It does next steps on each core:
///
/// 1 - Initializes the [`Executor`] with provided [`Config`];
///
/// 2 - Spawns `local_future` using `creator`;
///
/// 3 - Runs the [`Executor`].
///
/// # Example
///
/// ## High-performance echo server
///
/// ```no_run
/// use orengine::{run_on_all_cores_with_config, local_executor};
/// use orengine::runtime::Config;
/// use orengine::io::{full_buffer, AsyncBind, AsyncAccept};
/// use orengine::net::{Stream, TcpListener, TcpStream};
///
/// async fn handle_stream<S: Stream>(mut stream: S) {
///     loop {
///         stream.poll_recv().await.unwrap();
///         let mut buf = full_buffer();
///         buf.set_len_to_capacity();
///         let n = stream.recv(&mut buf).await.unwrap();
///         if n == 0 {
///             break;
///         }
///         stream.send_all(&buf.slice(..n)).await.unwrap();
///     }
/// }
///
/// fn main() {
///     run_on_all_cores_with_config(|| async {
///         let mut listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
///         loop {
///             let (stream, _) = listener.accept().await.unwrap();
///             local_executor().spawn_local(async move {
///                 handle_stream(stream).await;
///             });
///         }
///     }, Config::default().set_numbers_of_thread_workers(0).disable_work_sharing());
/// }
/// ```
#[allow(clippy::missing_panics_doc, reason = "Only std::thread can panic here")]
pub fn run_on_all_cores_with_config<Fut, F>(creator: F, cfg: Config)
where
    Fut: Future<Output = ()> + 'static,
    F: 'static + Clone + Send + Fn() -> Fut,
{
    let mut cores = utils::core::get_core_ids().unwrap();
    for core in cores.drain(1..) {
        let creator = creator.clone();
        std::thread::Builder::new()
            .name(format!("worker on core: {}", core.id))
            .spawn(move || {
                Executor::init_on_core_with_config(core, cfg);
                local_executor().spawn_local(creator());
                local_executor().run();
            })
            .expect("failed to create worker thread");
    }

    Executor::init_on_core(cores[0]);
    local_executor().spawn_local(creator());
    local_executor().run();
}

/// It does next steps on each core:
///
/// 1 - Initializes the [`Executor`];
///
/// 2 - Spawns `local` future using `creator`;
///
/// 3 - Runs the [`Executor`].
///
/// # Example
///
/// ## High-performance echo server
///
/// ```no_run
/// use orengine::{run_on_all_cores, local_executor};
/// use orengine::io::{full_buffer, AsyncBind, AsyncAccept};
/// use orengine::net::{Stream, TcpListener, TcpStream};
///
/// async fn handle_stream<S: Stream>(mut stream: S) {
///     loop {
///         stream.poll_recv().await.unwrap();
///         let mut buf = full_buffer();
///         buf.set_len_to_capacity();
///         let n = stream.recv(&mut buf).await.unwrap();
///         if n == 0 {
///             break;
///         }
///         stream.send_all(&buf.slice(..n)).await.unwrap();
///     }
/// }
///
/// fn main() {
///     run_on_all_cores(|| async {
///         let mut listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
///         loop {
///             let (stream, _) = listener.accept().await.unwrap();
///             local_executor().spawn_local(async move {
///                 handle_stream(stream).await;
///             });
///         }
///     });
/// }
/// ```
pub fn run_on_all_cores<Fut, F>(creator: F)
where
    Fut: Future<Output = ()> + 'static,
    F: 'static + Clone + Send + Fn() -> Fut,
{
    run_on_all_cores_with_config(creator, Config::default());
}
