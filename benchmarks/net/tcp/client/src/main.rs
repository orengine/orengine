use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use orengine::buf::buffer;
use orengine::io::{AsyncConnectStream, AsyncPollFd, AsyncRecv, AsyncSend};
use orengine::local::Local;
use orengine::runtime::local_executor;
use orengine::sync::LocalWaitGroup;
use orengine::utils::get_core_ids;
use orengine::Executor;
use smol::future;

const SERVER_ADDR: &str = "server:8083";

const PAR: usize = 512;
const N: usize = 5_200_000;
const COUNT: usize = N / PAR;
const TRIES: usize = 15;

fn bench_throughput() {
    macro_rules! bench_throughput_client {
        ($name:expr, $sleep:block, $code_of_test:block) => {
            let mut res = 0;
            for _ in 0..TRIES {
                $sleep
                let start = Instant::now();

                $code_of_test

                let rps = (N * 1000) / start.elapsed().as_millis() as usize;
                println!("Benchmark {} took: {}ms, RPS: {rps}", $name, start.elapsed().as_millis());

                res += rps;
            }

            println!("Average {} RPS: {}", $name, res / TRIES);
        };
    }

    fn bench_std() {
        bench_throughput_client!(
            "std client",
            {
                thread::sleep(Duration::from_secs(1));
            },
            {
                use std::io::{Read, Write};

                let mut joins = Vec::with_capacity(PAR);
                for _i in 0..PAR {
                    joins.push(thread::spawn(move || {
                        let mut conn = std::net::TcpStream::connect(SERVER_ADDR).unwrap();
                        let mut buf = [0u8; 1024];

                        for _ in 0..COUNT {
                            conn.write_all(b"ping").unwrap();
                            conn.read(&mut buf).unwrap();
                        }
                    }));
                }

                for join in joins {
                    join.join().unwrap();
                }
            }
        );
    }

    fn bench_smol() {
        let ex = smol::Executor::new();
        future::block_on(ex.run(async {
            bench_throughput_client!(
                "smol client",
                {
                    smol::Timer::after(Duration::from_secs(1)).await;
                },
                {
                    use smol::io::{AsyncReadExt, AsyncWriteExt};

                    let (tx, rx) = flume::bounded(PAR);

                    for _i in 0..PAR {
                        let tx = tx.clone();
                        ex.spawn(async move {
                            let mut conn =
                                smol::net::TcpStream::connect(SERVER_ADDR).await.unwrap();
                            let mut buf = [0u8; 1024];

                            for _ in 0..COUNT {
                                conn.write_all(b"ping").await.unwrap();
                                conn.read(&mut buf).await.unwrap();
                            }

                            tx.send_async(()).await.unwrap();
                        })
                        .detach();
                    }

                    for _ in 0..PAR {
                        rx.recv_async().await.unwrap();
                    }
                }
            );
        }));
    }

    fn bench_tokio() {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                bench_throughput_client!(
                    "tokio client",
                    {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    },
                    {
                        use tokio::io::{AsyncReadExt, AsyncWriteExt};

                        let (tx, rx) = flume::bounded(PAR);

                        for _i in 0..PAR {
                            let tx = tx.clone();
                            tokio::spawn(async move {
                                let mut conn =
                                    tokio::net::TcpStream::connect(SERVER_ADDR).await.unwrap();
                                let mut buf = [0u8; 1024];

                                for _ in 0..COUNT {
                                    conn.write_all(b"ping").await.unwrap();
                                    conn.read(&mut buf).await.unwrap();
                                }

                                tx.send_async(()).await.unwrap();
                            });
                        }

                        for _ in 0..PAR {
                            rx.recv_async().await.unwrap();
                        }
                    }
                );
            });
    }

    fn bench_async_std() {
        async_std::task::block_on(async {
            bench_throughput_client!(
                "async-std client",
                {
                    async_std::task::sleep(Duration::from_secs(1)).await;
                },
                {
                    use async_std::io::{ReadExt, WriteExt};

                    let (tx, rx) = flume::bounded(PAR);

                    for _i in 0..PAR {
                        let tx = tx.clone();
                        async_std::task::spawn(async move {
                            let mut conn = async_std::net::TcpStream::connect(SERVER_ADDR)
                                .await
                                .unwrap();
                            let mut buf = [0u8; 1024];

                            for _ in 0..COUNT {
                                conn.write_all(b"ping").await.unwrap();
                                conn.read(&mut buf).await.unwrap();
                            }

                            tx.send_async(()).await.unwrap();
                        });
                    }

                    for _ in 0..PAR {
                        rx.recv_async().await.unwrap();
                    }
                }
            );
        });
    }

    fn bench_orengine() {
        use std::sync::{Condvar, Mutex};

        let start_wg = Arc::new((Mutex::new(-1), Condvar::new()));
        let end_wg = Arc::new((Mutex::new(0), Condvar::new()));

        #[inline(always)]
        async fn start_client(
            number_of_cores: usize,
            start_wg: Arc<(Mutex<isize>, Condvar)>,
            end_wg: Arc<(Mutex<usize>, Condvar)>,
        ) {
            let par = PAR / number_of_cores;

            for i in 0..TRIES as isize {
                let mut guard = start_wg.0.lock().unwrap();
                while *guard != i {
                    guard = start_wg.1.wait(guard).unwrap();
                }
                drop(guard);

                let wg = Local::new(LocalWaitGroup::new());

                for _ in 0..par {
                    wg.get_mut().add(1);
                    let wg = wg.clone();
                    local_executor().spawn_local(async move {
                        let mut stream = orengine::net::TcpStream::connect(SERVER_ADDR)
                            .await
                            .unwrap();
                        for _ in 0..COUNT {
                            stream.send_all(b"ping").await.unwrap();

                            stream.poll_recv().await.unwrap();
                            let mut buf = buffer();
                            buf.set_len_to_cap();
                            stream.recv(&mut buf).await.unwrap();
                        }

                        wg.get_mut().done();
                    });
                }

                wg.get_mut().wait().await;

                let mut guard = end_wg.0.lock().unwrap();
                *guard += 1;
                if *guard == (i as usize + 1) * number_of_cores {
                    drop(guard);
                    end_wg.1.notify_all();
                }
            }
        }

        let cores = get_core_ids().unwrap();
        let number_of_cores = cores.len();

        for i in 0..number_of_cores {
            let core = cores[i];
            let start_wg = start_wg.clone();
            let end_wg = end_wg.clone();
            thread::spawn(move || {
                let ex = Executor::init_on_core(core);
                let _ = ex.run_and_block_on_local(start_client(number_of_cores, start_wg, end_wg));
            });
        }

        let mut res = 0;

        for i in 0..TRIES {
            let start = Instant::now();
            let mut start_guard = start_wg.0.lock().unwrap();
            *start_guard += 1;
            drop(start_guard);
            start_wg.1.notify_all();

            let mut end_guard = end_wg.0.lock().unwrap();
            while *end_guard != (i + 1) * number_of_cores {
                end_guard = end_wg.1.wait(end_guard).unwrap();
            }

            let rps = (N * 1000) / start.elapsed().as_millis() as usize;
            println!("Benchmark orengine took: {}ms, RPS: {rps}", start.elapsed().as_millis());

            res += rps;
            thread::sleep(Duration::from_secs(1));
        }

        println!("Average orengine RPS: {}", res / TRIES);
    }

    let client = std::env::var("CLIENT").expect("Environment variable 'CLIENT' not set");

    match client.as_str() {
        "std" => bench_std(),
        "tokio" => bench_tokio(),
        "async_std" => bench_async_std(),
        "smol" => bench_smol(),
        "orengine" => bench_orengine(),
        _ => {
            println!("Unknown client: {}, use one of: std, smol, tokio, async_std, orengine", client);
            std::process::exit(1);
        }
    }
}

fn main() {
    bench_throughput();
}
