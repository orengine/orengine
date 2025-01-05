use std::io;
use std::path::{Path, PathBuf};

/// Synonymous with os paths type.
pub(crate) type OsPath = PathBuf;

/// Synonymous with os paths type pointer.
pub(crate) type OsPathPtr = *const PathBuf;

/// Get os path from path.
#[inline(always)]
pub(crate) fn get_os_path(path: &Path) -> io::Result<OsPath> {
    Ok(path.to_path_buf())
}

/// Gets a pointer to os path.
pub(crate) fn get_os_path_ptr(path: &OsPath) -> OsPathPtr {
    path
}
