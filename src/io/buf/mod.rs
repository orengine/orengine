//! This module contains [`Buffer`] and [`BufPool`].
//! Read [`Buffer`] and [`BufPool`] for more information.
pub use buf_pool::*;
pub use buffer::*;
pub use fixed_io_buffer::*;
pub use sendable_buffer::*;
pub use slice::*;
pub use with_buffer::*;

pub mod buf_pool;
pub mod buffer;
pub mod fixed_io_buffer;
pub(crate) mod linux;
pub mod sendable_buffer;
pub mod slice;
mod tests;
pub mod with_buffer;
