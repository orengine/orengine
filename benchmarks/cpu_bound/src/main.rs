#![allow(internal_features)]

mod tools;

use orengine::runtime::{local_executor, stop_all_executors};
use orengine::sync::{local_scope, LocalChannel};
use orengine::Executor;
use smol::future;
use std::hint::black_box;
use std::thread;
use tools::bench;

static ORENGINE_INIT_ONCE: std::sync::Once = std::sync::Once::new();

fn init_orengine_cpu_bound() {
    let cfg = orengine::runtime::Config::default()
        .disable_work_sharing()
        .disable_io_worker()
        .set_numbers_of_thread_workers(0);
    ORENGINE_INIT_ONCE.call_once(move || {
        Executor::init_with_config(cfg);
    });
}

fn bench_create_task_and_yield() {
    const LARGE_SIZE: usize = 9000;

    bench("async_std small create_task_and_yield", |mut b| {
        async_std::task::block_on(async move {
            b.iter_async(|| async {
                let a = async_std::task::spawn(async {
                    async_std::task::yield_now().await;
                    black_box(0)
                })
                .await;
                black_box(a);
            })
            .await;
        });
    });

    bench("tokio small create_task_and_yield", |mut b| {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(async move {
                b.iter_async(|| async {
                    let a = tokio::spawn(async {
                        tokio::task::yield_now().await;
                        black_box(0)
                    })
                    .await
                    .unwrap();
                    black_box(a);
                })
                .await;
            });
    });

    bench("smol small create_task_and_yield", |mut b| {
        future::block_on(smol::LocalExecutor::new().run(async move {
            b.iter_async(|| async {
                let a = smol::spawn(async {
                    future::yield_now().await;
                    black_box(0)
                })
                .await;
                black_box(a);
            })
            .await;
        }));
    });

    bench("orengine small create_task_and_yield", |mut b| {
        init_orengine_cpu_bound();
        local_executor().spawn_local(async move {
            b.iter_async(|| async {
                let ret_ch = LocalChannel::bounded(0);
                local_scope(|scope| async {
                    scope.spawn(async {
                        orengine::yield_now().await;
                        let _ = ret_ch.send(0).await;
                    });

                    let _ = black_box(ret_ch.recv().await);
                })
                .await;
            })
            .await;
            stop_all_executors();
        });
        local_executor().run();
    });

    bench("sync small create_task_and_yield", |mut b| {
        b.iter(|| {
            let a = thread::spawn(|| {
                thread::yield_now();
                black_box(1)
            })
            .join()
            .unwrap();
            black_box(a);
        });
    });

    bench("async_std large create_task_and_yield", |mut b| {
        async_std::task::block_on(async move {
            b.iter_async(|| async {
                let a = async_std::task::spawn(async {
                    let a = black_box([0u8; LARGE_SIZE]);
                    async_std::task::yield_now().await;
                    black_box(a.len())
                })
                .await;
                black_box(a);
            })
            .await;
        });
    });

    bench("tokio large create_task_and_yield", |mut b| {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(async move {
                b.iter_async(|| async {
                    let a = tokio::spawn(async {
                        let a = black_box([0u8; LARGE_SIZE]);
                        tokio::task::yield_now().await;
                        black_box(a.len())
                    })
                    .await
                    .unwrap();
                    black_box(a);
                })
                .await;
            });
    });

    bench("smol large create_task_and_yield", |mut b| {
        future::block_on(smol::LocalExecutor::new().run(async move {
            b.iter_async(|| async {
                let a = smol::spawn(async {
                    let a = black_box([0u8; LARGE_SIZE]);
                    future::yield_now().await;
                    black_box(a.len())
                })
                .await;
                black_box(a);
            })
            .await;
        }));
    });

    bench("orengine large create_task_and_yield", |mut b| {
        init_orengine_cpu_bound();
        local_executor().spawn_local(async move {
            b.iter_async(|| async {
                let ret_ch = LocalChannel::bounded(0);
                local_scope(|scope| async {
                    scope.spawn(async {
                        let a = black_box([0u8; LARGE_SIZE]);
                        orengine::yield_now().await;
                        let _ = ret_ch.send(black_box(a.len())).await;
                    });

                    let _ = black_box(ret_ch.recv().await);
                })
                .await;
            })
            .await;
            stop_all_executors();
        });
        local_executor().run();
    });

    bench("sync large create_task_and_yield", |mut b| {
        b.iter(|| {
            let a = thread::spawn(|| {
                let a = black_box([0u8; LARGE_SIZE]);
                thread::yield_now();
                black_box(a.len())
            })
            .join()
            .unwrap();
            black_box(a);
        });
    });
}

fn bench_yield_task() {
    const NUMBER_TASKS: usize = 10;
    const YIELDS: usize = 10_000;
    const YIELDS_PER_TASK: usize = YIELDS / NUMBER_TASKS;

    bench("async_std task switch", |mut b| {
        async_std::task::block_on(async move {
            b.iter_async(|| async {
                let (tx, rx) = async_std::channel::bounded(NUMBER_TASKS);
                for _ in 0..NUMBER_TASKS {
                    let tx = tx.clone();
                    async_std::task::spawn(async move {
                        for _ in 0..YIELDS_PER_TASK {
                            async_std::task::yield_now().await;
                        }

                        tx.send(()).await.unwrap();
                    });
                }

                for _ in 0..NUMBER_TASKS {
                    rx.recv().await.unwrap();
                }
            })
            .await;
        });
    });

    bench("tokio task switch", |mut b| {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(async move {
                b.iter_async(|| async {
                    let (tx, mut rx) = tokio::sync::mpsc::channel(NUMBER_TASKS);
                    for _ in 0..NUMBER_TASKS {
                        let tx = tx.clone();
                        tokio::spawn(async move {
                            for _ in 0..YIELDS_PER_TASK {
                                tokio::task::yield_now().await;
                            }

                            tx.send(()).await.unwrap();
                        });
                    }

                    for _ in 0..NUMBER_TASKS {
                        rx.recv().await.unwrap();
                    }
                })
                .await;
            });
    });

    bench("smol task switch", |mut b| {
        future::block_on(smol::LocalExecutor::new().run(async move {
            b.iter_async(|| async {
                let (tx, rx) = smol::channel::bounded(NUMBER_TASKS);
                for _ in 0..NUMBER_TASKS {
                    let tx = tx.clone();
                    smol::spawn(async move {
                        for _ in 0..YIELDS_PER_TASK {
                            future::yield_now().await;
                        }

                        tx.send(()).await.unwrap();
                    })
                    .detach();
                }

                for _ in 0..NUMBER_TASKS {
                    rx.recv().await.unwrap();
                }
            })
            .await;
        }));
    });

    bench("orengine task switch", |mut b| {
        init_orengine_cpu_bound();
        local_executor()
            .run_and_block_on_local(async move {
                b.iter_async(|| async {
                    local_scope(|scope| async {
                        for _ in 0..NUMBER_TASKS {
                            scope.exec(async move {
                                for _ in 0..YIELDS_PER_TASK {
                                    orengine::yield_now().await;
                                }
                            });
                        }
                    })
                    .await;
                })
                .await;
            })
            .unwrap();
    });

    bench("sync task switch", |mut b| {
        b.iter(|| {
            thread::scope(|scope| {
                for _ in 0..NUMBER_TASKS {
                    scope.spawn(move || {
                        for _ in 0..YIELDS_PER_TASK {
                            thread::yield_now();
                        }
                    });
                }
            });
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
            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap()
                .block_on(async move {
                    b.iter_async(|| async {
                        let mutex = tokio::sync::Mutex::new(0);
                        for _ in 0..N {
                            let mut guard = mutex.lock().await;
                            *guard += 1;
                        }
                    })
                    .await;
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
                })
                .await;
            }));
        });
    }

    fn bench_orengine() {
        bench("orengine naive mutex", |mut b| {
            init_orengine_cpu_bound();
            local_executor()
                .run_and_block_on_local(async move {
                    b.iter_async(|| async {
                        let mutex = orengine::sync::NaiveMutex::new(0);
                        for _ in 0..N {
                            let mut guard = mutex.lock().await;
                            *guard += 1;
                        }
                    })
                    .await;
                    stop_all_executors();
                })
                .unwrap();
        });

        bench("orengine mutex", |mut b| {
            init_orengine_cpu_bound();
            local_executor()
                .run_and_block_on_local(async move {
                    b.iter_async(|| async {
                        let mutex = orengine::sync::mutex::Mutex::new(0);
                        for _ in 0..N {
                            let mut guard = mutex.lock().await;
                            *guard += 1;
                        }
                    })
                    .await;
                    stop_all_executors();
                })
                .unwrap();
        });
    }

    bench_std();
    bench_tokio();
    bench_smol();
    bench_orengine();
}

fn main() {
    bench_create_task_and_yield();
    bench_mutex();
    bench_yield_task();
}
