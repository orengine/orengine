//! This module contains [`Buffer`] and [`BufPool`].
//! Read [`Buffer`] and [`BufPool`] for more information.
#[cfg(target_os = "linux")]
pub use buffer::Buffer;
#[cfg(not(target_os = "linux"))]
pub use other_os::buffer::Buffer;

pub mod buf_pool;
pub mod buffer;
pub(crate) mod linux;
mod tests;
