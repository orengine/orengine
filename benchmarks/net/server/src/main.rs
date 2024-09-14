use std::thread;
use orengine::buf::buffer;
use orengine::Executor;
use orengine::utils::{get_core_ids, CoreId};

const ADDR: &str = "server:8083";

fn std_server() {
    println!("Using std.");
    #[inline(always)]
    fn handle_client(mut stream: std::net::TcpStream) {
        use std::io::{Read, Write};

        let mut buf = [0u8; 4096];
        loop {
            let n = stream.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            stream.write_all(&buf[..n]).unwrap();
        }
    }

    let mut listener = std::net::TcpListener::bind(ADDR).unwrap();
    while let Ok((stream, _)) = listener.accept() {
        thread::spawn(|| { handle_client(stream)});
    }
}

fn may() {
    println!("Using may.");

    #[inline(always)]
    fn handle_client(mut stream: may::net::TcpStream) {
        use std::io::{Read, Write};
        let mut buf = vec![0u8; 4096];
        loop {
            let n = stream.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            stream.write_all(&buf[..n]).unwrap();
        }
    }

    let mut listener = may::net::TcpListener::bind(ADDR).unwrap();
    while let Ok((stream, _)) = listener.accept() {
        may::go!(|| { handle_client(stream)});
    }
}

fn tokio() {
    println!("Using tokio.");
    #[inline(always)]
    async fn handle_client(mut stream: tokio::net::TcpStream) {
        use tokio::io::{AsyncWriteExt};

        loop {
            stream.readable().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let n = match stream.try_read(&mut buf) {
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(_) => break,
            };
            if n == 0 {
                break;
            }
            stream.write_all(&buf[..n]).await.unwrap();
        }
    }

    tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap().block_on(async {
        let mut listener = tokio::net::TcpListener::bind(ADDR).await.unwrap();
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                handle_client(stream).await
            });
        }
    });
}

fn async_std() {
    println!("Using async-std.");
    #[inline(always)]
    async fn handle_client(mut stream: async_std::net::TcpStream) {
        use async_std::io::{ReadExt, WriteExt};

        let mut buf = vec![0u8; 4096];
        loop {
            let n = stream.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            stream.write_all(&buf[..n]).await.unwrap();
        }
    }

    async_std::task::block_on(async {
        let mut listener = async_std::net::TcpListener::bind(ADDR).await.unwrap();
        while let Ok((stream, _)) = listener.accept().await {
            async_std::task::spawn(async move {
                handle_client(stream).await
            });
        }
    });
}

fn smol() {
    println!("Using smol.");
    #[inline(always)]
    async fn handle_client(mut stream: smol::net::TcpStream) {
        use smol::io::{AsyncReadExt, AsyncWriteExt};

        let mut buf = vec![0u8; 4096];
        loop {
            let n = stream.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            stream.write_all(&buf[..n]).await.unwrap();
        }
    }

    smol::future::block_on(smol::Executor::new().run(async {
        let mut listener = smol::net::TcpListener::bind(ADDR).await.unwrap();
        while let Ok((stream, _)) = listener.accept().await {
            smol::spawn(async move {
                handle_client(stream).await
            }).detach();
        }
    }));
}

fn orengine() {
    println!("Using orengine.");

    use orengine::io::{AsyncBind, AsyncAccept};

    #[inline(always)]
    async fn handle_client<S: orengine::net::Stream>(mut stream: S) {
        loop {
            stream.poll_recv().await.unwrap();
            let mut buf = buffer();
            buf.set_len_to_cap();
            let n = stream.recv(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            stream.send_all(&buf[..n]).await.unwrap();
        }
    }

    fn run_server(core_id: CoreId) {
        let ex = Executor::init_on_core(core_id);
        let _ = ex.run_and_block_on(async {
            let mut listener = orengine::net::TcpListener::bind(ADDR).await.unwrap();
            while let Ok((stream, _)) = listener.accept().await {
                orengine::local_executor().spawn_local(async move {
                    handle_client(stream).await
                });
            }
        });
    }

    let cores = get_core_ids().unwrap();
    for i in 1..cores.len() {
        let core = cores[i];
        thread::spawn(move || {
            run_server(core);
        });
    }
    run_server(cores[0]);
}

fn main() {
    std_server();
    //orengine();
}