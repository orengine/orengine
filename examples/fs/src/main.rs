use orengine::buf::buffer;
use orengine::fs::{DirBuilder, File, OpenOptions};
use orengine::io::{AsyncRead, AsyncWrite};
use orengine::{asyncify, Executor};

fn main() {
    Executor::init()
        .run_and_block_on_local(async {
            let _ = DirBuilder::new()
                .mode(0o777)
                .recursive(true)
                .create("./foo/bar")
                .await;
            let open_options = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .mode(0o777)
                .truncate(true);
            let mut file = File::open("./foo/bar/test.txt", &open_options)
                .await
                .unwrap();

            file.write_all(b"Hello, world!").await.unwrap(); // non-positioned write

            let mut buf = buffer();
            buf.set_len(5);
            file.pread_exact(&mut buf, 7).await.unwrap(); // positioned read

            assert_eq!(buf, b"world");

            // Now Orengine doesn't support read_dir, but you can use `std::fs::read_dir`
            // in async runtime with `asyncify`.
            asyncify!(|| {
                let res = std::fs::read_dir("./foo/bar").unwrap();
                for entry in res {
                    let entry = entry.unwrap();
                    let path = entry.path();
                    println!("{:?}", path); // "./foo/bar/test.txt"
                }
            })
            .await;
        })
        .expect("Execution failed");
}
