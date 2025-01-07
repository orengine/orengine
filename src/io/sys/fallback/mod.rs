pub(crate) mod io_call;
pub(crate) mod mio_poller;
pub(crate) mod open_options;
pub(crate) mod operations;
pub(crate) mod os_message_header;
pub(crate) mod os_path;
#[cfg(feature = "fallback_thread_pool")]
mod with_thread_pool;
#[cfg(not(feature = "fallback_thread_pool"))]
mod worker;

#[cfg(feature = "fallback_thread_pool")]
pub(crate) use with_thread_pool::worker_with_thread_pool::FallbackWorker;

#[cfg(not(feature = "fallback_thread_pool"))]
pub(crate) use worker::FallbackWorker;
