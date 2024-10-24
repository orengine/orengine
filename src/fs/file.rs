use crate::fs::OpenOptions;
use crate::io::fallocate::AsyncFallocate;
use crate::io::open::Open;
use crate::io::remove::Remove;
use crate::io::rename::Rename;
use crate::io::sync_all::AsyncSyncAll;
use crate::io::sync_data::AsyncSyncData;
use crate::io::sys::OsPath::{get_os_path, OsPath};
use crate::io::sys::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use crate::io::{AsyncClose, AsyncRead, AsyncWrite};
use crate::runtime::local_executor;
use std::io::{Error, Result};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::{io, mem};

/// An object providing access to an [`open`](File::open) file on the filesystem.
/// An instance of a File can be read and/ or written depending on what options it was opened with.
///
/// # Close
///
/// Files are automatically closed when they go out of scope.
/// Errors detected on closing are ignored by the implementation of Drop.
/// Use the method [`sync_all`](File::sync_all) if these errors must be manually handled.
///
/// # Examples
///
/// ```no_run
/// use orengine::fs::{File, OpenOptions};
///
/// # async fn foo() -> std::io::Result<()> {
/// let open_options = OpenOptions::new().read(true).write(true);
/// let file = File::open("example.txt", &open_options).await?;
/// # Ok(())
/// # }
/// ```
pub struct File {
    fd: RawFd,
}

impl File {
    /// Returns the file descriptor [`RawFd`] of the [`File`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::fs::{File, OpenOptions};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let file = File::open("foo.txt", &OpenOptions::new()).await?;
    /// let fd = file.fd();
    /// # Ok(())
    /// # }
    #[inline(always)]
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    /// Opens a file at the given path with the specified options.
    ///
    /// This function takes an asynchronous approach to file opening.
    /// The `as_path` argument specifies the path
    /// to the file, and `open_options` contains various settings such as read/write access,
    /// append mode, and more.
    ///
    /// # Errors
    ///
    /// This method will return an `Err` if:
    /// - The provided path is empty.
    /// - There is an issue converting the path to an OS-specific format.
    /// - The file could not be opened due to other I/O errors
    /// (e.g., permission denied, file not found).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::fs::{File, OpenOptions};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let open_options = OpenOptions::new().read(true).write(true);
    /// let file = File::open("foo.txt", &open_options).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn open<P: AsRef<Path>>(as_path: P, open_options: &OpenOptions) -> Result<Self> {
        let path = as_path.as_ref();
        if path == Path::new("") {
            return Err(Error::new(io::ErrorKind::InvalidInput, "path is empty"));
        }
        let os_path = match OsPath::new(path.as_os_str().as_bytes()) {
            Ok(path) => path,
            Err(err) => return Err(Error::new(io::ErrorKind::InvalidInput, err)),
        };
        let os_open_options = open_options.into_os_options()?;

        match Open::new(os_path, os_open_options).await {
            Ok(file) => Ok(file),
            Err(err) => Err(err),
        }
    }

    /// Renames a file from one path to another.
    ///
    /// This method renames the file at `old_path` to `new_path`. Both paths must be valid.
    ///
    /// # Errors
    ///
    /// This function will return an `Err` if:
    /// - Either `old_path` or `new_path` cannot be converted into an OS path.
    /// - The rename operation fails due to I/O issues such as permission errors or file not found.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use orengine::fs::{File, OpenOptions};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let file = File::rename("foo.txt", "bar.txt").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub async fn rename<OldPath, NewPath>(old_path: OldPath, new_path: NewPath) -> Result<()>
    where
        OldPath: AsRef<Path>,
        NewPath: AsRef<Path>,
    {
        let old_path = get_os_path(old_path.as_ref())?;
        let new_path = get_os_path(new_path.as_ref())?;
        Rename::new(old_path, new_path).await
    }

    /// Removes (deletes) the file at the specified path.
    ///
    /// This method asynchronously deletes the file located at `path`.
    /// If the file does not exist,
    /// or if the operation fails for any other reason, an `Err` is returned.
    ///
    /// # Errors
    ///
    /// This function will return an `Err` if:
    /// - The provided path cannot be converted into an OS path.
    /// - The file removal operation fails due to I/O issues such as permission errors
    /// or file not found.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use orengine::fs::{File, OpenOptions};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let file = File::remove("foo.txt").await?;
    /// assert!(!Path::new("foo.txt").exists());
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub async fn remove<P: AsRef<Path>>(path: P) -> Result<()> {
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
            fd: file.into_raw_fd(),
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

impl AsyncFallocate for File {}

impl AsyncSyncAll for File {}

impl AsyncSyncData for File {}

impl AsyncRead for File {}

impl AsyncWrite for File {}

impl AsyncClose for File {}

impl Drop for File {
    fn drop(&mut self) {
        let close_future = self.close();
        local_executor().exec_local_future(async {
            close_future.await.expect("Failed to close file");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::buf::buffer;
    use crate::fs::test_helper::{create_test_dir_if_not_exist, is_exists, TEST_DIR_PATH};
    use std::fs::create_dir;
    use std::path::PathBuf;

    #[orengine_macros::test_local]
    fn test_file_create_write_read_pread_pwrite_remove_close() {
        let test_file_dir_path: &str = &(TEST_DIR_PATH.to_string() + "/test_file/");

        create_test_dir_if_not_exist();

        let file_path = {
            let mut file_path_ = PathBuf::from(test_file_dir_path);
            let _ = create_dir(test_file_dir_path);
            file_path_.push("test.txt");
            file_path_
        };
        let options = OpenOptions::new()
            .write(true)
            .read(true)
            .truncate(true)
            .create(true);
        let mut file = match File::open(&file_path, &options).await {
            Ok(file) => file,
            Err(err) => panic!("Can't open (create) file: {}", err),
        };

        assert!(is_exists(file_path.clone()));

        let mut buf = buffer();
        const MSG: &[u8] = b"Hello, world!";
        buf.append(MSG);

        match file.write_all(buf.as_ref()).await {
            Ok(_) => (),
            Err(err) => panic!("Can't write file: {}", err),
        }

        match file.read_exact(buf.as_mut()).await {
            Ok(_) => assert_eq!(buf.as_ref(), MSG),
            Err(err) => panic!("Can't read file: {}", err),
        }

        buf.clear();
        buf.append("great World!".as_bytes());
        match file.pwrite_all(buf.as_ref(), 7).await {
            Ok(_) => (),
            Err(err) => panic!("Can't pwrite file: {}", err),
        }

        buf.clear();
        buf.set_len(MSG.len() + 6);
        match file.read_exact(buf.as_mut()).await {
            Ok(_) => assert_eq!(buf.as_ref(), b"Hello, great World!"),
            Err(err) => panic!("Can't read file: {}", err),
        }

        match File::rename(
            test_file_dir_path.to_string() + "test.txt",
            test_file_dir_path.to_string() + "test2.txt",
        )
        .await
        {
            Ok(_) => assert!(is_exists(test_file_dir_path.to_string() + "/test2.txt")),
            Err(err) => panic!("Can't rename file: {}", err),
        }

        match File::remove(test_file_dir_path.to_string() + "/test2.txt").await {
            Ok(_) => assert!(!is_exists(file_path)),
            Err(err) => panic!("Can't remove file: {}", err),
        }

        std::fs::remove_dir("./test/test_file").expect("failed to remove test file dir");
    }
}
