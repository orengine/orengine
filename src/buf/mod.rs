//! This module contains [`Buffer`] and [`BufPool`].
//! Read [`Buffer`] and [`BufPool`] for more information.
pub mod buf_pool;
pub mod buffer;

pub use self::buffer::Buffer;
pub use self::buf_pool::{BufPool, buffer, buf_pool};