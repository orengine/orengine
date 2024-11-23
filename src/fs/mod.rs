//! The `fs` module provides asynchronous file system operations.
//!
//! This module includes tools for working with files, directories,
//! and various file system utilities.
//! It offers abstractions for opening files with options, creating directories, and performing
//! common file operations in a non-blocking, async manner.
//!
//! # Example
//!
//! ```rust
//! use orengine::fs::{File, OpenOptions, DirBuilder, remove_file};
//!
//! # async fn foo() -> std::io::Result<()> {
//! let options = OpenOptions::new().read(true).write(true);
//! let file = File::open("example.txt", &options).await?;
//! remove_file("example.txt").await?;
//! # Ok(())
//! # }
//! ```

pub mod dir_builder;
pub mod file;
pub mod open_options;
pub mod shortcuts;
#[cfg(test)]
pub(crate) mod test_helper;

pub use dir_builder::DirBuilder;
pub use file::File;
pub use open_options::OpenOptions;
pub use shortcuts::*;
