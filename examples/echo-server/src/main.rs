use orengine::io::full_buffer;
use orengine::net::{Listener, Stream, TcpListener};
use orengine::{local_executor, run_on_all_cores};

async fn handle_stream<S: Stream>(mut stream: S) {
    loop {
        stream.poll_recv().await.expect("poll failed");

        let mut buf = full_buffer();
        let n = stream.recv(&mut buf).await.expect("recv failed");
        if n == 0 {
            break;
        }

        stream.send_all(&buf.slice(..n)).await.expect("send failed");
    }
}

async fn run_listener<L: Listener>() {
    let mut listener = L::bind("127.0.0.1:8080").await.expect("bind failed");

    while let Ok((stream, _addr)) = listener.accept().await {
        local_executor().spawn_local(handle_stream(stream));
    }
}

fn main() {
    run_on_all_cores(run_listener::<TcpListener>);
}
