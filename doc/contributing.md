# CONTRIBUTING

**Thank you for helping to make Orengine better!**

**Before the pull request, do the following:**

* Check a code quality via `cargo clippy`, `cargo doc` and `cargo fmt --all --check`;
*   Also check on other platforms:

    **Example**

    * Run for Windows: `cargo build --target x86_64-pc-windows-msvc`, `cargo build --all-features --target x86_64-pc-windows-msvc`, `cargo clippy --target x86_64-pc-windows-msvc`, `cargo doc --target x86_64-pc-windows-msvc`;
    * Run for macOS: `cargo build --target x86_64-apple-darwin`, `cargo build --all-features --target x86_64-apple-darwin`, `cargo clippy --target x86_64-apple-darwin`, `cargo doc --target x86_64-apple-darwin`;
    * Run for Linux: `cargo build --target x86_64-unknown-linux-gnu`, `cargo build --all-features --target x86_64-unknown-linux-gnu`, `cargo clippy --target x86_64-unknown-linux-gnu`, `cargo doc --target x86_64-unknown-linux-gnu`.
* Run `cargo test`, `cargo test --all-features`, `cargo test --release` and `cargo test --release --all-features`;
* If needed, update examples and benchmarks;
* Run benchmarks in `benchmarks` directory and compare with the previous version.
