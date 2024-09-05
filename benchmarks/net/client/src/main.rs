use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::{Duration, Instant};
use smol::future;
use orengine::io::{AsyncConnectStream, AsyncPollFd, AsyncRecv, AsyncSend};
use orengine::{sleep, Executor};
use orengine::buf::buffer;
use orengine::local::Local;
use orengine::runtime::{end, local_executor};
use orengine::sync::LocalWaitGroup;
use orengine::utils::get_core_ids;

const ADDR_ENGINE: &str = "engine:8081";
const ADDR_SERVER: &str = "server:8080";
const ADDR_ASYNC_SERVER: &str = "async:8083";
const CLIENT_SERVER: &str = "client:8079";

static HANDLE: AtomicUsize = AtomicUsize::new(0);

// async fn client() {
//     use tokio::net::TcpStream;
//     use tokio::io::AsyncReadExt;
//     tokio::time::sleep(Duration::from_secs(3)).await;
//     for _i in 0..20_000 {
//         tokio::spawn(async {
//             let mut stream = TcpStream::connect(ADDR_ENGINE).await.expect("failed to connect");
//             println!("handle: {}", HANDLE.fetch_add(1, SeqCst) + 2);
//             stream.read(&mut [0; 1]).await.unwrap();
//             panic!("asds");
//         });
//     }
//
//     let mut stream = TcpStream::connect(CLIENT_SERVER).await.expect("failed to connect");
//     stream.read(&mut [0; 1]).await.unwrap();
//     panic!("asds");
// }

// #[tokio::main(flavor = "current_thread")]
// async fn main() {
//     use tokio::net::TcpStream;
//     use tokio::io::AsyncReadExt;
//     println!("started");
//
//     // tokio::spawn(async {
//     //     client().await;
//     // });
//
//     let listener = tokio::net::TcpListener::bind(CLIENT_SERVER).await.unwrap();
//
//     loop {
//         let (stream, _) = listener.accept().await.unwrap();
//         tokio::spawn(async move {
//             println!("Handle: {}", HANDLE.fetch_add(1, SeqCst) + 2);
//             let mut stream = stream;
//             stream.read(&mut [0; 1]).await.unwrap();
//             panic!("server panic");
//         });
//     }
// }
//
const PAR: usize = 516;
const N: usize = 10_400 / 2;
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
            { thread::sleep(Duration::from_secs(1)); },
            {
                use std::io::{Read, Write};

                let mut joins = Vec::with_capacity(PAR);
                for _i in 0..PAR {
                    joins.push(thread::spawn(move || {
                        let mut conn = std::net::TcpStream::connect(ADDR_ASYNC_SERVER).unwrap();
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

    fn smol() {
        let ex = smol::Executor::new();
        future::block_on(ex.run(async {
            bench_throughput_client!(
                "smol client",
                { smol::Timer::after(Duration::from_secs(1)).await; },
                {
                    use smol::io::{AsyncReadExt, AsyncWriteExt};

                    let (tx, rx) = flume::bounded(PAR);

                    for _i in 0..PAR {
                        let tx = tx.clone();
                        ex.spawn(async move {
                            let mut conn = smol::net::TcpStream::connect(ADDR_ASYNC_SERVER).await.unwrap();
                            let mut buf = [0u8; 1024];

                            for _ in 0..COUNT {
                                conn.write_all(b"ping").await.unwrap();
                                conn.read(&mut buf).await.unwrap();
                            }

                            tx.send_async(()).await.unwrap();
                        }).detach();
                    }

                    for _ in 0..PAR {
                        rx.recv_async().await.unwrap();
                    }
                }
            );
        }));
    }

    fn tokio() {
        tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap().block_on(async {
            bench_throughput_client!(
                "tokio client",
                { tokio::time::sleep(Duration::from_secs(1)).await; },
                {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};

                    let (tx, rx) = flume::bounded(PAR);

                    for _i in 0..PAR {
                        let tx = tx.clone();
                        tokio::spawn(async move {
                            let mut conn = tokio::net::TcpStream::connect(ADDR_ASYNC_SERVER).await.unwrap();
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

    fn async_std() {
        async_std::task::block_on(async {
            bench_throughput_client!(
                "async-std client",
                { async_std::task::sleep(Duration::from_secs(1)).await; },
                {
                    use async_std::io::{ReadExt, WriteExt};

                    let (tx, rx) = flume::bounded(PAR);

                    for _i in 0..PAR {
                        let tx = tx.clone();
                        async_std::task::spawn(async move {
                            let mut conn = async_std::net::TcpStream::connect(ADDR_ASYNC_SERVER).await.unwrap();
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

    fn orengine() {
        #[inline(always)]
        async fn start_client(number_of_cores: usize, counter: Arc<AtomicUsize>) {
            let count = COUNT / number_of_cores;
            let par = PAR / number_of_cores;
            let mut total_rps = 0;

            for _ in 0..TRIES {
                let wg = Local::new(LocalWaitGroup::new());
                let start = Instant::now();

                for _ in 0..par {
                    wg.get_mut().add(1);
                    let wg = wg.clone();
                    local_executor().spawn_local(async move {
                        for _ in 0..count {
                            let mut stream = orengine::net::TcpStream::connect(ADDR_ASYNC_SERVER).await.unwrap();
                            stream.send_all(b"ping").await.unwrap();

                            stream.poll_recv().await.unwrap();
                            let mut buf = buffer();
                            stream.recv(&mut buf).await.unwrap();
                        }

                        wg.get_mut().done();
                    });
                }

                wg.get_mut().wait().await;

                let elapsed_ms = start.elapsed().as_millis();
                let rps = N as u128 / elapsed_ms;
                total_rps += rps;
                let current = counter.fetch_add(1, Ordering::Relaxed) + 1;
                if current % number_of_cores == 0 {
                    println!("orengine took {elapsed_ms} milliseconds, rps: {rps}");
                    if current == number_of_cores * TRIES {
                        println!("Average orengine rps: {}", total_rps / TRIES as u128);
                        end();
                    }
                }
                sleep(Duration::from_secs(1)).await;
            }
        }

        let cores = get_core_ids().unwrap();
        let number_of_cores = cores.len();
        let counter = Arc::new(AtomicUsize::new(0));

        for i in 1..number_of_cores {
            let core = cores[i];
            let counter = counter.clone();
            thread::spawn(move || {
                let ex = Executor::init_on_core(core);
                ex.run_and_block_on(start_client(number_of_cores, counter)).unwrap();
            });
        }

        let ex = Executor::init_on_core(cores[0]);
        ex.spawn_local(start_client(number_of_cores, counter));
        ex.run();
    }

    orengine();
}

fn main() {
    bench_throughput();
}