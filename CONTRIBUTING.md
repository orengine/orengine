__Thank you for helping to make Orengine better!__

__Before the pull request, do the following:__

- Check a code quality via `cargo clippy`, `cargo doc` and `cargo fmt --all --check;

- Run `cargo test` and `cargo test --release`;

- If needed, update examples and benchmarks;

- Run benchmarks in `benchmarks` directory and compare with the previous version.

You can test code on other platforms with using Dockerfiles in the `root` directory:

- on `linux`: `docker build -f Dockerfile_test_linux -t test_linux .; docker run --rm test_linux`;

- on `macos`: `docker build -f Dockerfile_test_macos -t test_macos .; docker run --rm test_macos`;

- on `windows`: `docker build -f Dockerfile_test_windows -t test_windows .; docker run --rm test_windows`;

- on `freebsd`: `docker build -f Dockerfile_test_freebsd -t test_freebsd .; docker run --rm test_freebsd`