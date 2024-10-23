# Benchmarks

This directory contains the results of various benchmarks.

## CPU-Bounded Benchmarks

### Create task and yield

This benchmark measures the overhead of creating a task, returning from it and yielding. This shows well how expensive
asynchronous abstractions of the engine are. This test has two options. In the first case, the tasks contain only one
number and return it. In the second case, the tasks contain a buffer that contains 9000 bytes on the stack and return
its length.
__Less is better__

__All__

![cpu_bound/images/create_task_and_yield.png](cpu_bound/images/create_task_and_yield.png)

__Favorites only__

![cpu_bound/images/create_task_and_yield_favorites.png](cpu_bound/images/create_task_and_yield_favorites.png)

### Task switch

This benchmark measures the overhead of switching between tasks.
__Less is better__

![cpu_bound/images/task_switch.png](cpu_bound/images/task_switch.png)

### Mutex lock and unlock

This benchmark measures the overhead of locking and unlocking a mutex. This test shows that an asynchronous Mutex can
run just as fast as a synchronous one. So don't be afraid to use orengine::sync::Mutex in your tasks!
__Less is better__

![cpu_bound/images/mutex_lock_unlock.png](cpu_bound/images/mutex_lock_unlock.png)

### Memory usage per task

This benchmark measures the memory usage per task. This was measured by creating 10,000,000 sleeping tasks, so the
result cannot be less than 16 bytes (required for std::time::Instant). In fact, it cannot be less than 32 bytes, since
you need to keep at least a wide pointer to the Future or even the Layout.
__Less is better__

__All__

![cpu_bound/images/memory_usage_per_10m_tasks_all.png](cpu_bound/images/memory_usage_per_10m_tasks_all.png)

__Favorites only__

![cpu_bound/images/memory_usage_per_10m_tasks_favorites_only.png](cpu_bound/images/memory_usage_per_10m_tasks_favorites_only.png)

## Network Benchmarks

### Echo server

This benchmark measures the TCP echo server throughput. The benchmark below used std::net for the client. For this
benchmark a 12-cpu machine was used and only 1024 connections were made.
__More is better__

![net/tcp/images/echo_server.png](net/tcp/images/echo_server.png)