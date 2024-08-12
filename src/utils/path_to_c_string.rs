use std::ffi::CString;
use std::io::{Error, ErrorKind};
use std::path::Path;

pub fn path_to_c_string(path: impl AsPath + Sized) -> Result<CString, Error> {
    match CString::new(path.as_ref().as_os_str().as_encoded_bytes()) {
        Ok(path) => Ok(path),
        Err(_) => Err(Error::new(ErrorKind::InvalidInput, "file name contained an unexpected NUL byte")),
    }
}