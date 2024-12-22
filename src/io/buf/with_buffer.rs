use crate::io::{buf_pool, SendableBuffer};
use crate::runtime::executor::get_local_executor_ref;
use crate::runtime::{update_current_task_locality, Locality};
use std::future::Future;

/// Allows `shared` [`Task`](crate::runtime::Task) to use a [`Buffer`](crate::io::Buffer).
///
/// It is possible because it gets a future spawner that must spawn a `local` future.
///
/// [`SendableBuffer`] that is provided to the future spawner have a length equal to 0.
/// Use [`with_any_len_buffer`] to get [`SendableBuffer`] with an any length or
/// use [`with_full_buffer`] to get [`SendableBuffer`] with a length equal to
/// [`BufPool::default_buffer_capacity`](buf_pool::BufPool::default_buffer_capacity).
///
/// # Safety
///
/// The future spawner spawns `local` future that is never moved to another thread.
///
/// # Panics
///
/// If the spawned future is moved to another thread in `debug` mode. In release mode
/// it causes undefined behavior.
///
/// # Example
///
/// ```no_run
/// use orengine::Executor;
/// use orengine::fs::{File, OpenOptions};
/// use orengine::io::{with_buffer, AsyncWrite};
///
/// async fn work_that_can_move_current_task() {}
///
/// Executor::init().run_and_block_on_shared(async {
///     // Here task is `shared`
///     let mut file = File::open("./foo.txt", &OpenOptions::new().write(true).create(true)).await.unwrap();
///     
///     work_that_can_move_current_task().await;
///
///     with_buffer(|mut buf| async move {
///         // Here task is `local`
///         buf.append(b"Hello world!");
///         file.write_all(&buf).await.unwrap();
///     }).await;
///
///     // Here task is `shared`
///
///     work_that_can_move_current_task().await;
/// }).unwrap();
/// ```
#[allow(clippy::future_not_send, reason = "It is used in `shared` tasks.")]
pub async fn with_buffer<Ret, Fut, F>(f: F) -> Ret
where
    Fut: Future<Output = Ret> + Send,
    F: Send + FnOnce(SendableBuffer) -> Fut,
{
    with_any_len_buffer(|mut buffer| async move {
        unsafe { buffer.set_len_unchecked(0) };

        f(buffer).await
    })
    .await
}

/// Allows `shared` [`Task`](crate::runtime::Task) to use a [`Buffer`](crate::io::Buffer).
///
/// It is possible because it gets a future spawner that must spawn a `local` future.
///
/// [`SendableBuffer`] that is provided to the future spawner have a length equal
/// to [`BufPool::default_buffer_capacity`](buf_pool::BufPool::default_buffer_capacity).
/// Use [`with_any_len_buffer`] to get [`SendableBuffer`] with any a length or
/// use [`with_buffer`] to get [`SendableBuffer`] with a length equal to 0.
///
/// # Safety
///
/// The future spawner spawns `local` future that is never moved to another thread.
///
/// # Panics
///
/// If the spawned future is moved to another thread in `debug` mode. In release mode
/// it causes undefined behavior.
///
/// # Example
///
/// ```no_run
/// use orengine::Executor;
/// use orengine::fs::{File, OpenOptions};
/// use orengine::io::{with_buffer, AsyncRead};
///
/// async fn work_that_can_move_current_task() {}
///
/// Executor::init().run_and_block_on_shared(async {
///     // Here task is `shared`
///     let mut file = File::open("./foo.txt", &OpenOptions::new().read(true).write(true).create(true)).await.unwrap();
///     
///     work_that_can_move_current_task().await;
///
///     with_buffer(|mut buf| async move {
///         // Here task is `local`
///         let n = file.read(&mut buf).await.unwrap();
///         println!("{} bytes read", n);
///     }).await;
///
///     // Here task is `shared`
///
///     work_that_can_move_current_task().await;
/// }).unwrap();
/// ```
#[allow(clippy::future_not_send, reason = "It is used in `shared` tasks.")]
pub async fn with_full_buffer<Ret, Fut, F>(f: F) -> Ret
where
    Fut: Future<Output = Ret> + Send,
    F: Send + FnOnce(SendableBuffer) -> Fut,
{
    with_any_len_buffer(|mut buffer| async move {
        buffer.set_len_to_capacity();

        f(buffer).await
    })
    .await
}

/// Allows `shared` [`Task`](crate::runtime::Task) to use a [`Buffer`](crate::io::Buffer).
///
/// It is possible because it gets a future spawner that must spawn a `local` future.
///
/// [`SendableBuffer`] that is provided to the future spawner have an any length.
/// Use [`with_full_buffer`] to get [`SendableBuffer`] with a length equal to
/// [`BufPool::default_buffer_capacity`](buf_pool::BufPool::default_buffer_capacity) or use
/// [`with_buffer`] to get [`SendableBuffer`] with a length equal to 0.
///
/// # Safety
///
/// The future spawner spawns `local` future that is never moved to another thread.
///
/// # Panics
///
/// If the spawned future is moved to another thread in `debug` mode. In release mode
/// it causes undefined behavior.
///
/// # Example
///
/// ```no_run
/// use orengine::Executor;
/// use orengine::fs::{File, OpenOptions};
/// use orengine::io::{with_any_len_buffer, AsyncRead};
///
/// async fn work_that_can_move_current_task() {}
///
/// Executor::init().run_and_block_on_shared(async {
///     // Here task is `shared`
///     let mut file = File::open("./foo.txt", &OpenOptions::new().read(true).write(true).create(true)).await.unwrap();
///     
///     work_that_can_move_current_task().await;
///
///     with_any_len_buffer(|mut buf| async move {
///         buf.set_len(100).unwrap();
///         // Here task is `local`
///         let n = file.read(&mut buf).await.unwrap();
///         println!("{} bytes read", n);
///     }).await;
///
///     // Here task is `shared`
///
///     work_that_can_move_current_task().await;
/// }).unwrap();
/// ```
#[allow(clippy::future_not_send, reason = "It is used in `shared` tasks.")]
pub async fn with_any_len_buffer<Ret, Fut, F>(f: F) -> Ret
where
    Fut: Future<Output = Ret> + Send,
    F: Send + FnOnce(SendableBuffer) -> Fut,
{
    debug_assert!(
        get_local_executor_ref().is_some(),
        "Executor is not initialized."
    );

    let buffer = unsafe { SendableBuffer::from_buffer(buf_pool().get_buffer_with_any_len()) };
    #[cfg(debug_assertions)]
    let parent_executor_id = crate::local_executor().id();
    let future = f(buffer);

    unsafe { update_current_task_locality(Locality::local()).await };

    let ret = future.await;

    unsafe { update_current_task_locality(Locality::shared()).await };

    #[cfg(debug_assertions)]
    {
        let current_executor_id = crate::local_executor().id();
        assert_eq!(
            parent_executor_id, current_executor_id,
            "Fatal error: buffer from `with_any_len_buffer` or `with_buffer` or `with_full_buffer`\
             was used with `shared` future. Therefore, buffer is not returned to the its pool. Use \
             These functions only with `local` futures (read docs for examples)."
        );
    }

    ret
}

#[cfg(test)]
mod tests {
    use crate as orengine;
    use crate::io::{with_any_len_buffer, with_buffer, with_full_buffer, FixedBuffer};

    #[orengine::test::test_shared]
    fn test_with_buffer() {
        with_any_len_buffer(|mut buffer| async move {
            buffer.set_len(0).unwrap();
            buffer.append(b"Hello, world!");
            assert_eq!(buffer.as_bytes(), b"Hello, world!");
        })
        .await;

        with_buffer(|mut buffer| async move {
            assert!(buffer.is_empty());
            buffer.append(b"Hello, world!");
            assert_eq!(buffer.as_bytes(), b"Hello, world!");
        })
        .await;

        with_full_buffer(|mut buffer| async move {
            assert!(buffer.is_full());
            buffer.clear();
            buffer.append(b"Hello, world!");
            assert_eq!(buffer.as_bytes(), b"Hello, world!");
        })
        .await;
    }
}
