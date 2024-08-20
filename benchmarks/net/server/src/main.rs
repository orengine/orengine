use std::thread;

const ADDR: &str = "async:8083";

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

fn main() {
    std_server();
}