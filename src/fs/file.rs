use std::{io, mem};
use std::path::{Path};
use std::io::{Error, Result};
use std::os::unix::ffi::OsStrExt;
use crate::fs::{OpenOptions};
use crate::io::{AsyncClose, AsyncRead, AsyncWrite, AsPath};
use crate::io::open::Open;
use crate::io::remove::Remove;
use crate::io::rename::Rename;
use crate::io::sys::{AsRawFd, RawFd, FromRawFd, IntoRawFd};
use crate::io::sys::OsPath::{get_os_path, OsPath};
use crate::runtime::local_executor;

// TODO docs
pub struct File {
    fd: RawFd
}

impl File {
    /// Returns the state_ptr of the [`File`].
    ///
    /// Uses for low-level work with the scheduler. If you don't know what it is, don't use it.
    #[inline(always)]
    pub fn fd(&mut self) -> RawFd {
        self.fd
    }

    pub async fn open<P: AsPath>(as_path: P, open_options: &OpenOptions) -> Result<Self> {
        let path = as_path.as_ref();
        if path == Path::new("") {
            return Err(Error::new(io::ErrorKind::InvalidInput, "path is empty"));
        }
        let os_path = match OsPath::new(path.as_os_str().as_bytes()) {
            Ok(path) => path,
            Err(err) => return Err(Error::new(io::ErrorKind::InvalidInput, err))
        };
        let os_open_options = open_options.into_os_options()?;

        match Open::new(os_path, os_open_options).await {
            Ok(file) => Ok(file),
            Err(err) => Err(err)
        }
    }

    #[inline(always)]
    pub async fn rename<OldPath, NewPath>(old_path: OldPath, new_path: NewPath) -> Result<()>
        where OldPath: AsPath, NewPath: AsPath
    {
        let old_path = get_os_path(old_path.as_ref())?;
        let new_path = get_os_path(new_path.as_ref())?;
        Rename::new(old_path, new_path).await
    }

    #[inline(always)]
    pub async fn remove<P: AsPath>(path: P) -> Result<()> {
        let path = get_os_path(path.as_ref())?;
        Remove::new(path).await
    }
}

impl Into<std::fs::File> for File {
    fn into(self) -> std::fs::File {
        let fd = self.fd;
        mem::forget(self);

        unsafe { std::fs::File::from_raw_fd(fd) }
    }
}

impl From<std::fs::File> for File {
    fn from(file: std::fs::File) -> Self {
        Self {
            fd: file.into_raw_fd()
        }
    }
}

impl FromRawFd for File {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self { fd }
    }
}

impl AsRawFd for File {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl AsyncRead for File {}

impl AsyncWrite for File {}

impl AsyncClose for File {}

impl Drop for File {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().spawn_local(async {
            close_future.await.expect("Failed to close file");
        });
    }
}

#[cfg(test)]
mod tests {
    use std::fs::create_dir;
    use std::mem;
    use std::path::PathBuf;
    use crate::buf::buffer;
    use super::*;
    use crate::fs::test_helper::{create_test_dir_if_not_exist, is_exists, TEST_DIR_PATH};
    use crate::runtime::create_local_executer_for_block_on;

    #[test]
    fn test_file_create_write_read_pread_pwrite_remove_close() {
        let test_file_dir_path_: &str = &(TEST_DIR_PATH.to_string() + "/test_file/");
        let test_file_dir_path = unsafe {mem::transmute::<&str, &'static str>(test_file_dir_path_)};
        create_test_dir_if_not_exist();
        create_local_executer_for_block_on(async move {
            let file_path = {
                let mut file_path_ = PathBuf::from(test_file_dir_path);
                let _ = create_dir(test_file_dir_path);
                file_path_.push("test.txt");
                file_path_
            };
            let options = OpenOptions::new().write(true).read(true).truncate(true).create(true);
            let mut file = match File::open(file_path.clone(), &options).await {
                Ok(file) => file,
                Err(err) => panic!("Can't open (create) file: {}", err)
            };

            assert!(is_exists(file_path.clone()));

            let mut buf = buffer();
            const MSG: &[u8] = b"Hello, world!";
            buf.append(MSG);

            match file.write_all(buf.as_ref()).await {
                Ok(_) => (),
                Err(err) => panic!("Can't write file: {}", err)
            }

            match file.read_exact(buf.as_mut()).await {
                Ok(_) => assert_eq!(buf.as_ref(), MSG),
                Err(err) => panic!("Can't read file: {}", err)
            }

            buf.clear();
            buf.append("great World!".as_bytes());
            match file.pwrite_all(buf.as_ref(), 7).await {
                Ok(_) => (),
                Err(err) => panic!("Can't pwrite file: {}", err)
            }

            buf.clear();
            buf.set_len(MSG.len() + 6);
            match file.read_exact(buf.as_mut()).await {
                Ok(_) => assert_eq!(buf.as_ref(), b"Hello, great World!"),
                Err(err) => panic!("Can't read file: {}", err)
            }

            match File::rename(
                test_file_dir_path.to_string() + "test.txt",
                test_file_dir_path.to_string() + "test2.txt"
            ).await {
                Ok(_) => assert!(is_exists(test_file_dir_path.to_string() + "/test2.txt")),
                Err(err) => panic!("Can't rename file: {}", err)
            }

            match File::remove(test_file_dir_path.to_string() + "/test2.txt").await {
                Ok(_) => assert!(!is_exists(file_path)),
                Err(err) => panic!("Can't remove file: {}", err)
            }

            std::fs::remove_dir("./test/test_file").expect("failed to remove test file dir");
        });
    }
}