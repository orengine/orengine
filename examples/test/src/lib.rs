use orengine::sleep;
use orengine::sync::Mutex;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

async fn awesome_async_local_fn() -> usize {
    sleep(Duration::from_millis(100)).await;

    42
}

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
    use orengine::sync::{Mutex, WaitGroup};
    use orengine::test::{
        run_test_and_block_on_global, run_test_and_block_on_local, sched_future_to_another_thread,
        ExecutorPool,
    };
    use std::sync::Arc;

    #[orengine::test::test_global]
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

    #[orengine::test::test_global]
    fn test_test_global_and_parallel_with_macro() {
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
    fn test_test_global_and_parallel_without_macro() {
        run_test_and_block_on_global(async {
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

    #[orengine::test::test_global]
    fn test_test_global_and_parallel_with_macro_and_manually_join() {
        let mutex = Arc::new(Mutex::new(0));
        let mut joins = Vec::with_capacity(10);

        for _ in 0..10 {
            let mutex = mutex.clone();

            let join_handle = ExecutorPool::sched_future(async move {
                awesome_async_fn_to_parallel_test(mutex).await;
            })
            .await;

            joins.push(join_handle);
        }

        for join_handle in joins {
            join_handle.join().await;
        }

        assert_eq!(*mutex.lock().await, 1000);
    }
}
