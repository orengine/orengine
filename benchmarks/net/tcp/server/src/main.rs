use orengine::buf::full_buffer;
use orengine::runtime::Config;
use orengine::utils::{get_core_ids, CoreId};
use orengine::Executor;
use std::sync::LazyLock;
use std::{io, thread};

static ADDR: LazyLock<String> = LazyLock::new(|| {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        println!("Server address is {} (given as second argument)", args[2]);

        args[2].clone()
    } else {
        println!("Server address is not specified, defaulting to localhost:8083");

        "localhost:8083".to_string()
    }
});

fn std_server() {
    println!("Using std.");
    #[inline(always)]
    fn handle_client(mut stream: std::net::TcpStream) -> io::Result<()> {
        use std::io::{Read, Write};

        let mut buf = [0u8; 4096];
        loop {
            let n = stream.read(&mut buf)?;
            if n == 0 {
                break;
            }

            stream.write_all(&buf[..n])?;
        }

        Ok(())
    }

    let listener = std::net::TcpListener::bind::<&str>(ADDR.as_ref()).unwrap();
    while let Ok((stream, _)) = listener.accept() {
        thread::spawn(|| handle_client(stream));
    }
}

fn may() {
    println!("Using may.");

    #[inline(always)]
    fn handle_client(mut stream: may::net::TcpStream) -> io::Result<()> {
        use std::io::{Read, Write};
        let mut buf = vec![0u8; 4096];
        loop {
            let n = stream.read(&mut buf)?;
            if n == 0 {
                break;
            }
            stream.write_all(&buf[..n])?;
        }

        Ok(())
    }

    let listener = may::net::TcpListener::bind::<&str>(ADDR.as_ref()).unwrap();
    while let Ok((stream, _)) = listener.accept() {
        may::go!(|| { handle_client(stream) });
    }
}

fn tokio() {
    println!("Using tokio.");
    #[inline(always)]
    async fn handle_client(mut stream: tokio::net::TcpStream) -> io::Result<()> {
        use tokio::io::AsyncWriteExt;

        loop {
            stream.readable().await?;
            let mut buf = vec![0u8; 4096];
            let n = match stream.try_read(&mut buf) {
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(_) => break,
            };
            if n == 0 {
                break;
            }
            stream.write_all(&buf[..n]).await?;
        }

        Ok(())
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let listener = tokio::net::TcpListener::bind::<&str>(ADDR.as_ref())
                .await
                .unwrap();
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(async move { handle_client(stream).await });
            }
        });
}

fn async_std() {
    println!("Using async-std.");
    #[inline(always)]
    async fn handle_client(mut stream: async_std::net::TcpStream) -> io::Result<()> {
        use async_std::io::{ReadExt, WriteExt};

        let mut buf = vec![0u8; 4096];
        loop {
            let n = stream.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            stream.write_all(&buf[..n]).await?;
        }

        Ok(())
    }

    async_std::task::block_on(async {
        let listener = async_std::net::TcpListener::bind::<&str>(ADDR.as_ref())
            .await
            .unwrap();
        while let Ok((stream, _)) = listener.accept().await {
            async_std::task::spawn(async move { handle_client(stream).await });
        }
    });
}

fn orengine() {
    println!("Using orengine.");

    use orengine::io::{AsyncAccept, AsyncBind};

    #[inline(always)]
    async fn handle_client<S: orengine::net::Stream>(mut stream: S) {
        loop {
            stream.poll_recv().await.unwrap();
            let mut buf = full_buffer();

            let n = stream.recv(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }

            stream.send_all(&buf[..n]).await.unwrap();
        }
    }

    fn run_server(core_id: CoreId) {
        let ex =
            Executor::init_on_core_with_config(core_id, Config::default().disable_work_sharing());
        let _ = ex.run_and_block_on_local(async {
            let mut listener = orengine::net::TcpListener::bind::<&str>(ADDR.as_ref())
                .await
                .unwrap();
            while let Ok((stream, _)) = listener.accept().await {
                orengine::local_executor().spawn_local(handle_client(stream));
            }
        });
    }

    let mut cores = get_core_ids().unwrap();
    for core in cores.drain(1..cores.len()) {
        thread::spawn(move || {
            run_server(core);
        });
    }
    run_server(cores[0]);
}

fn main() {
    let server = std::env::args().nth(1).expect("First argument (server name) is required. Use one of: std, may, tokio, async_std, orengine");
    match server.as_str() {
        "std" => std_server(),
        "tokio" => tokio(),
        "async_std" => async_std(),
        "may" => may(),
        "orengine" => orengine(),
        _ => {
            println!(
                "Unknown server: {}, use one of: std, may, tokio, async_std, orengine",
                server
            );
            std::process::exit(1);
        }
    }
}
