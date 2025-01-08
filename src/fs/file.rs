use crate::fs::OpenOptions;
use crate::io::close::AsyncFileClose;
use crate::io::fallocate::AsyncFallocate;
use crate::io::open::Open;
use crate::io::remove::Remove;
use crate::io::rename::Rename;
use crate::io::sync_all::AsyncSyncAll;
use crate::io::sync_data::AsyncSyncData;
use crate::io::sys::get_os_path;
use crate::io::sys::{AsFile, AsRawFile, FromRawFile, IntoRawFile, RawFile};
use crate::io::{AsyncRead, AsyncWrite};
use crate::runtime::local_executor;
use std::io::{Error, Result};
use std::mem::ManuallyDrop;
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
/// ```rust
/// use orengine::fs::{File, OpenOptions};
///
/// # async fn foo() -> std::io::Result<()> {
/// let open_options = OpenOptions::new().read(true).write(true);
/// let file = File::open("example.txt", &open_options).await?;
/// # Ok(())
/// # }
/// ```
pub struct File {
    raw_file: RawFile,
}

impl File {
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
    /// - The provided path is empty;
    /// - There is an issue converting the path to an OS-specific format;
    /// - The file could not be opened due to other I/O errors
    ///   (e.g., permission denied, file not found).
    ///
    /// # Example
    ///
    /// ```rust
    /// use orengine::fs::{File, OpenOptions};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// let open_options = OpenOptions::new().read(true).write(true);
    /// let file = File::open("foo.txt", &open_options).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn open<P: AsRef<Path> + Send>(
        as_path: P,
        open_options: &OpenOptions,
    ) -> Result<Self> {
        let path = as_path.as_ref();
        if path == Path::new("") {
            return Err(Error::new(io::ErrorKind::InvalidInput, "path is empty"));
        }
        let os_path = get_os_path(path)?;
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
    /// - Either `old_path` or `new_path` cannot be converted into an OS path;
    /// - The rename operation fails due to I/O issues such as permission errors or file not found.
    ///
    /// # Example
    ///
    /// ```rust
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
        OldPath: AsRef<Path> + Send,
        NewPath: AsRef<Path> + Send,
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
    /// - The provided path cannot be converted into an OS path;
    /// - The file removal operation fails due to I/O issues such as permission errors
    ///   or file not found.
    ///
    /// # Example
    ///
    /// ```rust
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
    pub async fn remove<P: AsRef<Path> + Send>(path: P) -> Result<()> {
        let path = get_os_path(path.as_ref())?;
        Remove::new(path).await
    }

    /// Executes a closure with a shared reference to the underlying `std::fs::File` object.
    ///
    /// It allows to call sync methods on the file from standard library.
    #[inline(always)]
    pub fn with_std_file<Ret, F: FnOnce(&std::fs::File) -> Ret>(&self, f: F) -> Ret {
        unsafe {
            let std_file = std::fs::File::from_raw_file(self.raw_file);
            let ret = f(&std_file);
            mem::forget(std_file);

            ret
        }
    }

    /// Executes a closure with a mutable reference to the underlying `std::fs::File` object.
    ///
    /// It allows to call sync methods on the file from standard library.
    #[inline(always)]
    pub fn with_std_mut_file<Ret, F: FnOnce(&mut std::fs::File) -> Ret>(&mut self, f: F) -> Ret {
        unsafe {
            let mut std_file = std::fs::File::from_raw_file(self.raw_file);
            let ret = f(&mut std_file);
            mem::forget(std_file);

            ret
        }
    }
}

impl From<File> for std::fs::File {
    fn from(file: File) -> Self {
        unsafe { Self::from_raw_file(ManuallyDrop::new(file).raw_file) }
    }
}

impl From<std::fs::File> for File {
    fn from(file: std::fs::File) -> Self {
        Self {
            raw_file: file.into_raw_file(),
        }
    }
}

#[cfg(unix)]
impl std::os::fd::IntoRawFd for File {
    fn into_raw_fd(self) -> std::os::fd::RawFd {
        ManuallyDrop::new(self).raw_file
    }
}

#[cfg(windows)]
impl std::os::windows::io::IntoRawHandle for File {
    fn into_raw_handle(self) -> RawFile {
        ManuallyDrop::new(self).raw_file
    }
}

impl IntoRawFile for File {}

#[cfg(unix)]
impl std::os::fd::AsRawFd for File {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.raw_file
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsRawHandle for File {
    fn as_raw_handle(&self) -> RawFile {
        self.raw_file
    }
}

impl AsRawFile for File {}

#[cfg(unix)]
impl std::os::fd::AsFd for File {
    fn as_fd(&self) -> std::os::fd::BorrowedFd {
        unsafe { std::os::fd::BorrowedFd::borrow_raw(self.raw_file) }
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsHandle for File {
    fn as_handle(&self) -> std::os::windows::io::BorrowedHandle {
        unsafe { std::os::windows::io::BorrowedHandle::borrow_raw(self.raw_file) }
    }
}

impl AsFile for File {}

#[cfg(unix)]
impl std::os::fd::FromRawFd for File {
    unsafe fn from_raw_fd(raw_fd: std::os::fd::RawFd) -> Self {
        Self { raw_file: raw_fd }
    }
}

#[cfg(windows)]
impl std::os::windows::io::FromRawHandle for File {
    unsafe fn from_raw_handle(raw_handle: RawFile) -> Self {
        Self {
            raw_file: raw_handle,
        }
    }
}

impl FromRawFile for File {}

impl AsyncFallocate for File {}

impl AsyncSyncAll for File {}

impl AsyncSyncData for File {}

impl AsyncRead for File {}

impl AsyncWrite for File {}

impl AsyncFileClose for File {}

unsafe impl Send for File {}

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
    use crate::fs::test_helper::{create_test_dir_if_not_exist, is_exists, TEST_DIR_PATH};
    use crate::io::{full_buffer, get_fixed_buffer, get_full_fixed_buffer, FixedBuffer};
    use std::fs::{create_dir, create_dir_all};
    use std::io::{Seek, SeekFrom};
    use std::path::PathBuf;

    #[orengine::test::test_local]
    fn test_file_create_write_read_pread_pwrite_remove_close_with_nonfixed() {
        const MSG: &[u8] = b"Hello, world!";

        let test_file_dir_path: &str = &(TEST_DIR_PATH.to_string() + "/test_file_nonfixed/");

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
            Err(err) => panic!("Can't open (create) file: {err}"),
        };

        assert!(is_exists(file_path.clone()));

        let mut buf = Vec::with_capacity(0);
        buf.extend(MSG);

        match file.write_all_bytes(buf.as_ref()).await {
            Ok(()) => (),
            Err(err) => panic!("Can't write file: {err}"),
        }

        file.with_std_mut_file(|file| file.seek(SeekFrom::Start(0)))
            .unwrap();
        match file.read_bytes_exact(buf.as_mut()).await {
            Ok(()) => assert_eq!(buf, MSG),
            Err(err) => panic!("Can't read file: {err}"),
        }

        buf.clear();
        buf.extend(b"great World!");
        match file.pwrite_all_bytes(buf.as_ref(), 7).await {
            Ok(()) => (),
            Err(err) => panic!("Can't pwrite file: {err}"),
        }

        buf.clear();
        buf.extend(b"Hello, great World!");

        match file.pread_bytes_exact(buf.as_mut(), 0).await {
            Ok(()) => assert_eq!(buf, b"Hello, great World!"),
            Err(err) => panic!("Can't read file: {err}"),
        }

        File::rename(
            test_file_dir_path.to_string() + "test.txt",
            test_file_dir_path.to_string() + "test2.txt",
        )
        .await
        .expect("Can't rename file");
        assert!(is_exists(test_file_dir_path.to_string() + "/test2.txt"));

        File::remove(test_file_dir_path.to_string() + "/test2.txt")
            .await
            .expect("Can't remove file");
        assert!(!is_exists(file_path));

        std::fs::remove_dir("./test/test_file_nonfixed").expect("failed to remove test file dir");
    }

    #[orengine::test::test_local]
    fn test_file_unpositional_read_write_with_nonfixed() {
        create_test_dir_if_not_exist();

        let test_file_dir_path: &str =
            &(TEST_DIR_PATH.to_string() + "/unpositional_file_nonfixed/");
        let file_path = {
            let mut file_path_ = PathBuf::from(test_file_dir_path);
            let _ = create_dir_all(test_file_dir_path);
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
            Err(err) => panic!("Can't open (create) file: {err}"),
        };

        let mut write_buf = vec![0u8; 4096];
        for i in 0..write_buf.capacity() {
            write_buf[i] = u8::try_from(i % 256).unwrap();
        }

        file.write_all_bytes(write_buf.as_ref()).await.unwrap();

        let mut read_buf = [0; 256];
        let mut read = 0usize;
        let mut read_file = File::open(&file_path, &OpenOptions::new().read(true))
            .await
            .unwrap();

        while read < write_buf.capacity() {
            let n = read_file.read_bytes(&mut read_buf).await.unwrap();
            assert_eq!(write_buf[read..read + n], read_buf[..n]);
            read += n;
        }

        let mut large_big_buff = full_buffer();
        read_file
            .with_std_mut_file(|file| file.seek(SeekFrom::Start(0)))
            .unwrap();
        read_file
            .read_bytes_exact(&mut large_big_buff[..write_buf.capacity()])
            .await
            .unwrap();

        assert_eq!(large_big_buff.as_ref(), write_buf);
    }

    #[orengine::test::test_local]
    fn test_file_create_write_read_pread_pwrite_remove_close_with_fixed() {
        const MSG: &[u8] = b"Hello, world!";

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
            Err(err) => panic!("Can't open (create) file: {err}"),
        };

        assert!(is_exists(file_path.clone()));

        let mut buf = get_fixed_buffer().await;
        buf.append(MSG);

        match file.write_all(&buf).await {
            Ok(()) => (),
            Err(err) => panic!("Can't write file: {err}"),
        }

        file.with_std_mut_file(|file| file.seek(SeekFrom::Start(0)))
            .unwrap();
        match file.read_exact(&mut buf).await {
            Ok(()) => assert_eq!(buf.as_ref(), MSG),
            Err(err) => panic!("Can't read file: {err}"),
        }

        buf.clear();
        buf.append(b"great World!");
        match file.pwrite_all(&buf, 7).await {
            Ok(()) => (),
            Err(err) => panic!("Can't pwrite file: {err}"),
        }

        buf.clear();
        buf.set_len(u32::try_from(b"Hello, great World!".len()).unwrap())
            .unwrap();
        match file.pread_exact(&mut buf, 0).await {
            Ok(()) => assert_eq!(buf.as_ref(), b"Hello, great World!"),
            Err(err) => panic!("Can't read file: {err}"),
        }

        File::rename(
            test_file_dir_path.to_string() + "test.txt",
            test_file_dir_path.to_string() + "test2.txt",
        )
        .await
        .expect("Can't rename file");
        assert!(is_exists(test_file_dir_path.to_string() + "/test2.txt"));

        File::remove(test_file_dir_path.to_string() + "/test2.txt")
            .await
            .expect("Can't remove file");
        assert!(!is_exists(file_path));

        std::fs::remove_dir("./test/test_file").expect("failed to remove test file dir");
    }

    #[orengine::test::test_local]
    fn test_file_unpositional_read_write_with_fixed() {
        let test_file_dir_path: &str = &(TEST_DIR_PATH.to_string() + "/unpositional_file/");

        create_test_dir_if_not_exist();

        let file_path = {
            let mut file_path_ = PathBuf::from(test_file_dir_path);
            let _ = create_dir_all(test_file_dir_path);
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
            Err(err) => panic!("Can't open (create) file: {err}"),
        };

        let mut write_buf = get_full_fixed_buffer().await;
        for i in 0..write_buf.capacity() as usize {
            write_buf[i] = u8::try_from(i % 256).unwrap();
        }

        file.write_all(&write_buf).await.unwrap();

        let mut read_buf = get_fixed_buffer().await;
        read_buf.set_len(256).unwrap();
        let mut read = 0;
        let mut read_file = File::open(&file_path, &OpenOptions::new().read(true))
            .await
            .unwrap();

        while read < write_buf.capacity() {
            let n = read_file.read(&mut read_buf).await.unwrap();
            assert_eq!(
                write_buf.as_bytes()[read as usize..read as usize + n as usize],
                read_buf.as_bytes()[..n as usize]
            );
            read += n;
        }

        let mut large_big_buff = full_buffer();
        assert_eq!(large_big_buff.len(), write_buf.capacity() as usize);
        read_file
            .with_std_mut_file(|file| file.seek(SeekFrom::Start(0)))
            .unwrap();
        read_file
            .read_bytes_exact(&mut large_big_buff[..write_buf.capacity() as usize])
            .await
            .unwrap();

        assert_eq!(large_big_buff.as_ref(), write_buf.as_ref());
    }
}
