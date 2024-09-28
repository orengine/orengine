use std::fmt::Debug;
use std::io::Write;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use orengine::io::AsyncWrite;
use orengine::runtime::{local_executor, Config};
use orengine::sync::WaitGroup;
use orengine::utils::{get_core_ids, CoreId};
use orengine::{asyncify, local_yield_now, stop_executor, Executor, Local};
use orengine::io::sync_data::AsyncSyncData;

fn main() {
    const N: usize = 20_000;
    const SIZE: usize = 100;
    const PAR: usize = 64;

    fn warn_up() {
        use std::io::Write;

        let start = Instant::now();
        for i in 0..15 {
            let mut file = {
                let dir_path = "./bench/std";
                std::fs::create_dir_all(dir_path).unwrap();
                let path = format!("{}/file_{}.txt", dir_path, i);
                std::fs::File::create(path).unwrap()
            };

            let buf = [0u8; SIZE];
            for _i in 0..N {
                file.write_all(&buf).unwrap();
            }
        }

        let elapsed = start.elapsed();
        let rps = ((N * 15) as f64 / elapsed.as_secs_f64()) as usize;
        println!("warn up: {elapsed:?}, rps: {rps}");
    }

    // fn tokio() {
    //     use tokio::io::AsyncWriteExt;
    //
    //     tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(async {
    //         let start = std::time::Instant::now();
    //         for i in 0..15 {
    //             let mut file = {
    //                 let dir_path = "./bench/tokio";
    //                 std::fs::create_dir_all(dir_path).unwrap();
    //                 let path = format!("{}/file_{}.txt", dir_path, i);
    //                 tokio::fs::File::create(path).await.unwrap()
    //             };
    //
    //             let buf = [0u8; SIZE];
    //             for _i in 0..N {
    //                 black_box(file.write_all(&buf).await.unwrap());
    //             }
    //         }
    //
    //         let elapsed = start.elapsed();
    //         let rps = (N * 15) as f64 / elapsed.as_secs_f64();
    //         println!("tokio: {elapsed:?}, rps: {rps}");
    //     });
    // }

    fn std() {
        use std::io::Write;

        let start = Instant::now();
        for i in 0..15 {
            let mut file = {
                let dir_path = "./bench/std";
                std::fs::create_dir_all(dir_path).unwrap();
                let path = format!("{}/file_{}.txt", dir_path, i);
                std::fs::File::create(path).unwrap()
            };

            let buf = [0u8; SIZE];
            for _i in 0..N {
                file.write_all(&buf).unwrap();
                //file.sync_data().unwrap();
            }
        }

        let elapsed = start.elapsed();
        let rps = ((N * 15) as f64 / elapsed.as_secs_f64()) as usize;
        println!("std: {elapsed:?}, rps: {rps}");
    }

    fn orengine() {
        let _ = Executor::init().run_and_block_on_local(async {
            let start = Instant::now();
            for i in 0..15 {
                let mut file = {
                    let dir_path = "./bench/my";
                    std::fs::create_dir_all(dir_path).unwrap();
                    let path = format!("{}/file_{}.txt", dir_path, i);
                    let open_options = orengine::fs::OpenOptions::new()
                        .append(true)
                        .create(true);
                    orengine::fs::File::open(path, &open_options).await.unwrap()
                };

                let buf = [0u8; SIZE];
                for _j in 0..N {
                    file.write_all(&buf).await.unwrap();
                    //file.sync_data().await.unwrap();
                }
            }

            let elapsed = start.elapsed();
            let rps = ((N * 15) as f64 / elapsed.as_secs_f64()) as usize;
            println!("orengine: {elapsed:?}, rps: {rps}");
        });
    }

    fn semi_orengine() {
        let _ = Executor::init().run_and_block_on_local(async {
            let start = Instant::now();
            for i in 0..15 {
                let buf = [0u8; SIZE];
                let local_file = Local::new({
                    let dir_path = "./bench/semi_orengine";
                    std::fs::create_dir_all(dir_path).unwrap();
                    let path = format!("{}/file_{}.txt", dir_path, i);
                    std::fs::File::create(path).unwrap()
                });
                
                for _j in 0..N {
                    let local_file = local_file.clone();
                    //asyncify!(move || {
                        local_file.get_mut().write_all(&buf).unwrap();
                        local_yield_now().await;
                    //}).await;
                }
            }

            let elapsed = start.elapsed();
            let rps = ((N * 15) as f64 / elapsed.as_secs_f64()) as usize;
            println!("semi orengine: {elapsed:?}, rps: {rps}");
        });
    }
    
    fn parallel_std() {
        use std::io::Write;

        let start = Instant::now();
        for i in 0..15 {
            let mut joins = Vec::new();
            for x in 0..PAR {
                joins.push(thread::spawn(move || {
                    let mut file = {
                        let dir_path = "./bench/std_parallel";
                        std::fs::create_dir_all(dir_path).unwrap();
                        let path = format!("{}/{x}file_{}.txt", dir_path, i);
                        std::fs::File::create(path).unwrap()
                    };

                    let buf = [0u8; SIZE];
                    for _i in 0..N {
                        file.write_all(&buf).unwrap();
                    }
                }));
            }

            for join in joins {
                join.join().unwrap();
            }
        }

        let elapsed = start.elapsed();
        let rps = (N * 15 * PAR) as f64 / elapsed.as_secs_f64();
        println!("std: {elapsed:?}, parallel: {PAR}, rps: {rps}");
    }

    fn parallel_orengine() {
        fn run_non_main_executor(core_id: CoreId) {
            let config = Config::default().set_work_sharing_level(1);
            Executor::init_on_core_with_config(core_id, config);
        }

        let start = Instant::now();
        for i in 0..15 {
            let wg = Arc::new(WaitGroup::new());
            let cores = get_core_ids().unwrap();

            for i in 1..cores.len() {
                let core = cores[i];
                thread::spawn(move || {
                    run_non_main_executor(core);
                });
            }

            let config = Config::default().set_work_sharing_level(1);
            let _ = Executor::init_on_core_with_config(cores[0], config)
                .run_and_block_on_global(async move {
                    let id = local_executor().id();
                    for x in 0..PAR {
                        let wg = wg.clone();
                        wg.add(1);
                        local_executor().spawn_global(async move {
                            let mut file = {
                                let dir_path = "./bench/my_parallel";
                                std::fs::create_dir_all(dir_path).unwrap();
                                let path = format!("{}/{x}file_{}.txt", dir_path, i);
                                let open_options = orengine::fs::OpenOptions::new()
                                    .append(true)
                                    .create(true);
                                orengine::fs::File::open(path, &open_options).await.unwrap()
                            };

                            let buf = [0u8; SIZE];
                            for _j in 0..N {
                                file.write_all(&buf).await.unwrap();
                            }
                            wg.done();
                        });
                    }

                    wg.wait().await;
                    stop_executor(id);
                });
            thread::sleep(Duration::from_millis(1));
        }

        let elapsed = start.elapsed();
        let rps = (N * 15 * PAR) as f64 / elapsed.as_secs_f64();
        println!("orengine: {elapsed:?}, parallel: {PAR}, rps: {rps}");
    }

    println!("FS bench started!\n");

    //warn_up();
    //println!();
    orengine();
    semi_orengine();
    std();
    //println!();
    //parallel_std();
    //parallel_orengine();

    println!("\nFS bench done!");
}
