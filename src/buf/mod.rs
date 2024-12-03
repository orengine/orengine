//! This module contains [`Buffer`] and [`BufPool`].
//! Read [`Buffer`] and [`BufPool`] for more information.
pub use buf_pool::*;
#[cfg(target_os = "linux")]
pub use buffer::*;
pub use io_buffer::*;
#[cfg(not(target_os = "linux"))]
pub use other_os::buffer::Buffer;

pub mod buf_pool;
pub mod buffer;
pub mod io_buffer;
pub(crate) mod linux;
mod tests;
