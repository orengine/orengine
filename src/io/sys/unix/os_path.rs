use std::ffi::CString;
use std::io::{Result, ErrorKind, Error};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

pub(crate) type OsPath = CString;

#[inline(always)]
pub(crate) fn get_os_path(path: &Path) -> Result<OsPath> {
    match CString::new(path.as_os_str().as_bytes()) {
        Ok(path) => Ok(path),
        Err(err) => Err(Error::new(ErrorKind::InvalidInput, err))
    }
}