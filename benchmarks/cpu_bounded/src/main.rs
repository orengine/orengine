#![allow(internal_features)]
#![feature(core_intrinsics)]

use std::intrinsics::black_box;
use std::thread;
use smol::future;
use orengine::Executor;
use orengine::runtime::{stop_all_executors, local_executor};
use orengine::sync::{local_scope, LocalChannel};
use tools::bench;

fn bench_yield_task() {
    const LARGE_SIZE: usize = 9000;

    bench("async_std small yield_task", |mut b| {
        async_std::task::block_on(async move {
            b.iter_async(|| async {
                let a = async_std::task::spawn(async {
                    async_std::task::yield_now().await;
                    black_box(0)
                }).await;
                black_box(a);
            }).await;
        });
    });

    bench("tokio small yield_task", |mut b| {
        tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(async move {
            b.iter_async(|| async {
                let a = tokio::spawn(async {
                    tokio::task::yield_now().await;
                    black_box(0)
                }).await.unwrap();
                black_box(a);
            }).await;
        });
    });

    bench("smol small yield_task", |mut b| {
        future::block_on(smol::LocalExecutor::new().run(async move {
            b.iter_async(|| async {
                let a= smol::spawn(async  {
                    future::yield_now().await;
                    black_box(0)
                }).await;
                black_box(a);
            }).await;
        }));
    });

    bench("orengine small yield_task", |mut b| {
        Executor::init().spawn_local(async move {
            b.iter_async(|| async {
                let ret_ch = LocalChannel::bounded(0);
                local_scope(|scope| async {
                    scope.spawn(async {
                        orengine::yield_now().await;
                        let _ = ret_ch.send(0).await;
                    });

                    let _ = black_box(ret_ch.recv().await);
                }).await;
            }).await;
            stop_all_executors();
        });
        local_executor().run();
    });

    bench("sync small yield_task", |mut b| {
        b.iter(|| {
            let a = thread::spawn(|| {
                thread::yield_now();
                black_box(1)
            }).join().unwrap();
            black_box(a);
        });
    });

    bench("async_std large yield_task", |mut b| {
        async_std::task::block_on(async move {
            b.iter_async(|| async {
                let a = async_std::task::spawn(async {
                    let a = black_box([0u8; LARGE_SIZE]);
                    async_std::task::yield_now().await;
                    black_box(a.len())
                }).await;
                black_box(a);
            }).await;
        });
    });

    bench("tokio large yield_task", |mut b| {
        tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(async move {
            b.iter_async(|| async {
                let a = tokio::spawn(async {
                    let a = black_box([0u8; LARGE_SIZE]);
                    tokio::task::yield_now().await;
                    black_box(a.len())
                }).await.unwrap();
                black_box(a);
            }).await;
        });
    });

    bench("smol large yield_task", |mut b| {
        future::block_on(smol::LocalExecutor::new().run(async move {
            b.iter_async(|| async {
                let a= smol::spawn(async {
                    let a = black_box([0u8; LARGE_SIZE]);
                    future::yield_now().await;
                    black_box(a.len())
                }).await;
                black_box(a);
            }).await;
        }));
    });

    bench("orengine large yield_task", |mut b| {
        Executor::init().spawn_local(async move {
            b.iter_async(|| async {
                let ret_ch = LocalChannel::bounded(0);
                local_scope(|scope| async {
                    scope.spawn(async {
                        let a = black_box([0u8; LARGE_SIZE]);
                        orengine::yield_now().await;
                        let _ = ret_ch.send(black_box(a.len())).await;
                    });

                    let _ = black_box(ret_ch.recv().await);
                }).await;
            }).await;
            stop_all_executors();
        });
        local_executor().run();
    });

    bench("sync large yield_task", |mut b| {
        b.iter(|| {
            let a = thread::spawn(|| {
                let a = black_box([0u8; LARGE_SIZE]);
                thread::yield_now();
                black_box(a.len())
            }).join().unwrap();
            black_box(a);
        });
    });
}

fn bench_mutex() {
    const N: usize = 20_000;

    fn bench_std() {
        bench("std mutex", |mut b| {
            b.iter(move || {
                let mutex = std::sync::Mutex::new(0);
                for _ in 0..N {
                    let mut guard = mutex.lock().unwrap();
                    *guard += 1;
                }
            });
        });
    }

    fn bench_tokio() {
        bench("tokio mutex", |mut b| {
            tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(async move {
                b.iter_async(|| async {
                    let mutex = tokio::sync::Mutex::new(0);
                    for _ in 0..N {
                        let mut guard = mutex.lock().await;
                        *guard += 1;
                    }
                }).await;
            });
        });
    }

    fn bench_smol() {
        bench("smol mutex", |mut b| {
            future::block_on(smol::LocalExecutor::new().run(async move {
                b.iter_async(|| async {
                    let mutex = smol::lock::Mutex::new(0);
                    for _ in 0..N {
                        let mut guard = mutex.lock().await;
                        *guard += 1;
                    }
                }).await;
            }));
        });
    }

    fn bench_orengine() {
        bench("orengine naive mutex", |mut b| {
            Executor::init().spawn_local(async move {
                b.iter_async(|| async {
                    let mutex = orengine::sync::NaiveMutex::new(0);
                    for _ in 0..N {
                        let mut guard = mutex.lock().await;
                        *guard += 1;
                    }
                }).await;
                stop_all_executors();
            });
            local_executor().run();
        });

        bench("orengine mutex", |mut b| {
            Executor::init().spawn_local(async move {
                b.iter_async(|| async {
                    let mutex = orengine::sync::mutex::Mutex::new(0);
                    for _ in 0..N {
                        let mut guard = mutex.lock().await;
                        *guard += 1;
                    }
                }).await;
                stop_all_executors();
            });
            local_executor().run();
        });
    }

    bench_std();
    bench_tokio();
    bench_smol();
    bench_orengine();
}

fn main() {
    //bench_yield_task();
    bench_mutex();
}