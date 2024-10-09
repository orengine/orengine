# This directory contains the results of the net/tcp benchmarks

## Echo server

This benchmark measures the TCP echo server throughput. The benchmark below used `std::net` for the client.
For this benchmark a 12-cpu machine was used and only 1024 connections were made.
__More is better__

![images/echo_server.png](images/echo_server.png)

To run it set __SERVER__ environment variable to server application with one of the following values:

- `std`
- `async-std`
- `tokio`
- `may`
- `orengine`

And set __CLIENT__ environment variable to client application with one of the following values:

- `std`
- `async-std`
- `tokio`
- `smol`
- `orengine`