//! This module contains [`Buffer`] and [`BufPool`].
//! Read [`Buffer`] and [`BufPool`] for more information.
pub use buf_pool::*;
pub use buffer::*;
pub use fixed_io_buffer::*;
pub use slice::*;

pub mod buf_pool;
pub mod buffer;
pub mod fixed_io_buffer;
pub(crate) mod linux;
pub mod slice;
mod tests;
