use orengine::sleep;
use orengine::sync::{AsyncMutex, Mutex};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[allow(dead_code)]
async fn awesome_async_local_fn() -> usize {
    sleep(Duration::from_millis(100)).await;

    42
}

#[allow(dead_code)]
async fn awesome_async_fn_to_parallel_test(mutex: Arc<Mutex<usize>>) {
    for _ in 0..100 {
        let sleep_for = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros()
            % 63;
        sleep(Duration::from_millis(sleep_for as u64)).await;
        *mutex.lock().await += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::{awesome_async_fn_to_parallel_test, awesome_async_local_fn};
    use orengine::sync::{AsyncMutex, AsyncWaitGroup, Mutex, WaitGroup};
    use orengine::test::{
        run_test_and_block_on_local, run_test_and_block_on_shared, sched_future_to_another_thread,
        ExecutorPool,
    };
    use std::sync::Arc;

    #[orengine::test::test_shared]
    fn test_local_with_macro() {
        let res = awesome_async_local_fn().await;
        assert_eq!(res, 42);
    }

    #[test]
    fn test_local_without_macro() {
        run_test_and_block_on_local(async {
            let res = awesome_async_local_fn().await;
            assert_eq!(res, 42);
        });
    }

    #[orengine::test::test_shared]
    fn test_test_shared_and_parallel_with_macro() {
        let mutex = Arc::new(Mutex::new(0));
        let wg = Arc::new(WaitGroup::new());

        for _ in 0..10 {
            let wg = wg.clone();
            let mutex = mutex.clone();

            wg.inc();
            sched_future_to_another_thread(async move {
                awesome_async_fn_to_parallel_test(mutex).await;
                wg.done();
            });
        }

        wg.wait().await;
        assert_eq!(*mutex.lock().await, 1000);
    }

    #[test]
    fn test_test_shared_and_parallel_without_macro() {
        run_test_and_block_on_shared(async {
            let mutex = Arc::new(Mutex::new(0));
            let wg = Arc::new(WaitGroup::new());

            for _ in 0..10 {
                let wg = wg.clone();
                let mutex = mutex.clone();

                wg.inc();
                sched_future_to_another_thread(async move {
                    awesome_async_fn_to_parallel_test(mutex).await;
                    wg.done();
                });
            }

            wg.wait().await;
            assert_eq!(*mutex.lock().await, 1000);
        });
    }

    #[orengine::test::test_shared]
    fn test_test_shared_and_parallel_with_macro_and_manually_join() {
        let mutex = Arc::new(Mutex::new(0));
        let mut handles = Vec::with_capacity(10);

        for _ in 0..10 {
            let mutex = mutex.clone();

            let join_handle = ExecutorPool::sched_future(async move {
                awesome_async_fn_to_parallel_test(mutex).await;
            })
            .await;

            handles.push(join_handle);
        }

        for handle in handles {
            handle.join().await;
        }

        assert_eq!(*mutex.lock().await, 1000);
    }
}
