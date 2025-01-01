use std::ffi::CString;
use std::io::{self, Error, ErrorKind};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// Synonymous with os path type.
pub(crate) type OsPath = CString;

/// Get os path from path.
#[inline(always)]
pub(crate) fn get_os_path(path: &Path) -> io::Result<OsPath> {
    match CString::new(path.as_os_str().as_bytes()) {
        Ok(path) => Ok(path),
        Err(err) => Err(Error::new(ErrorKind::InvalidInput, err)),
    }
}
