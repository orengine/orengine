use crate::io::{buf_pool, SendableBuffer};
use crate::runtime::executor::get_local_executor_ref;
use crate::runtime::{update_current_task_locality, Locality};
use std::future::Future;

// TODO docs here and docs above
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

// TODO docs here and docs above
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

// TODO docs here and docs above
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
