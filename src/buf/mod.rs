//! This module contains [`Buffer`] and [`BufPool`].
//! Read [`Buffer`] and [`BufPool`] for more information.
pub use self::buf_pool::{buf_pool, buffer, full_buffer, BufPool};
pub use self::buffer::Buffer;

pub mod buf_pool;
pub mod buffer;
