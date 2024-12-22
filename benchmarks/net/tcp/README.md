# This directory contains the results of the net/tcp benchmarks

## Echo server

This benchmark measures the TCP echo server throughput. The benchmark below used `std::net` for the client.
For this benchmark a 12-cpu machine was used and only 1024 connections were made.
__More is better__

![images/echo_server.png](images/echo_server.png)

# Run server

Run server with one argument with one of the following values:

- `std`
- `async-std`
- `tokio`
- `may`
- `orengine`

Second argument that is the server address (default is `localhost:8083`).

Example command: `cargo run --release orengine localhost:8083`

# Run client

Run client with one argument with one of the following values:

- `std`
- `async-std`
- `tokio`
- `smol`
- `orengine`

Second argument that is the server address (default is `localhost:8083`).

Third argument that is the number of messages (default is 5,200,000).

Fourth argument that is the number of connections (default is 512).

Example command: `cargo run --release orengine localhost:8083 5200000 512`