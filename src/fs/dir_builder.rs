use crate::io::create_dir::CreateDir;
use crate::io::sys::OsPath::get_os_path;
use smallvec::SmallVec;
use std::ffi::OsStr;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

#[derive(Debug)]
/// A builder used to create directories in various manners.
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use orengine::fs::DirBuilder;
///
/// # async fn foo() -> std::io::Result<()> {
/// DirBuilder::new()
///     .recursive(true)
///     .mode(0o777)
///     .create("foo/bar")
///     .await?;
///
/// assert!(Path::new("foo/bar").exists());
/// # Ok(())
/// # }
/// ```
pub struct DirBuilder {
    mode: u32,
    recursive: bool,
}

impl DirBuilder {
    /// Creates a new set of options with default mode/security settings for all
    /// platforms and also non-recursive.
    #[must_use]
    pub fn new() -> DirBuilder {
        DirBuilder {
            mode: 0o666,
            recursive: false,
        }
    }

    /// Indicates that directories should be created recursively, creating all
    /// parent directories. Parents that do not exist are created with the same
    /// security and permissions settings.
    ///
    /// This option defaults to `false`.
    pub fn recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    /// Sets the mode for the new directory
    pub fn mode(mut self, mode: u32) -> Self {
        self.mode = mode;
        self
    }

    /// Creates the specified directory with the options configured in this
    /// builder.
    ///
    /// It is considered an error if the directory already exists unless
    /// [`recursive mode`](DirBuilder::recursive) is enabled.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use orengine::fs::DirBuilder;
    ///
    /// # async fn foo() -> std::io::Result<()> {
    ///  DirBuilder::new().mode(0o777).create("foo").await?;
    ///  assert!(Path::new("foo").exists());
    /// # Ok(())
    /// # }
    pub async fn create<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        let path = path.as_ref();
        if self.recursive {
            Self::create_dir_all(path, self.mode).await
        } else {
            let path = get_os_path(path)?;
            CreateDir::new(path, self.mode).await
        }
    }

    /// Creates the specified directory and all of its parent components if they don't exist.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::path::Path;
    /// use orengine::fs::{create_dir_all};
    ///
    /// # async fn foo() -> std::io::Result<()> {
    /// create_dir_all("foo/bar").await?;
    /// assert!(Path::new("foo/bar").exists());
    /// # Ok(())
    /// # }
    async fn create_dir_all(path: &Path, mode: u32) -> io::Result<()> {
        #[inline(always)]
        fn get_offset<const STACK_CAP: usize>(
            offsets: &mut SmallVec<usize, STACK_CAP>,
            path: &Path,
        ) -> Result<usize, ()> {
            let mut path_index;
            if offsets.is_empty() {
                path_index = path.as_os_str().len() - 1
            } else {
                path_index = unsafe { *offsets.get_unchecked(offsets.len() - 1) - 1 };
            }

            let bytes = path.as_os_str().as_encoded_bytes();
            loop {
                if path_index == 1 || bytes[path_index] == b'.' || bytes[path_index] == b':' {
                    if bytes[path_index] == b'.' {
                        if path_index + 1 == bytes.len()
                            || bytes[path_index + 1] == std::path::MAIN_SEPARATOR as u8
                        {
                            break Err(());
                        }
                    } else {
                        break Err(());
                    }
                }

                if bytes[path_index] == std::path::MAIN_SEPARATOR as u8 {
                    offsets.push(path_index);
                    break Ok(path_index);
                }

                path_index -= 1;
            }
        }

        if path == Path::new("") {
            return Ok(());
        }

        let mut tmp_path = path;
        let mut tmp_mode = mode;
        let mut path_stack = SmallVec::<usize, 4>::new();

        loop {
            match CreateDir::new(get_os_path(tmp_path)?, tmp_mode).await {
                Ok(()) => {
                    if path_stack.is_empty() {
                        return Ok(());
                    }
                    if path_stack.len() == 1 {
                        tmp_mode = mode;
                        tmp_path = path;
                        path_stack.clear();
                        continue;
                    }
                    path_stack.pop();
                    unsafe {
                        tmp_path = Path::new(OsStr::from_bytes(
                            &path.as_os_str().as_encoded_bytes()
                                [..*path_stack.get_unchecked(path_stack.len() - 1)],
                        ))
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::NotFound => {
                    match get_offset(&mut path_stack, path) {
                        Ok(offset) => {
                            tmp_path = Path::new(OsStr::from_bytes(
                                &path.as_os_str().as_encoded_bytes()[..offset],
                            ));
                            tmp_mode = 0o777;
                        }
                        Err(_) => {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "failed to create path tree",
                            ));
                        }
                    }
                }
                Err(_) if path.is_dir() => {
                    if path_stack.is_empty() {
                        return Ok(());
                    }
                }
                Err(err) => {
                    return Err(err);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as orengine;
    use crate::fs::test_helper::{create_test_dir_if_not_exist, is_exists, TEST_DIR_PATH};
    use std::path::PathBuf;

    #[orengine_macros::test_local]
    fn test_dir_builder() {
        let dir_builder = DirBuilder::new();
        assert_eq!(dir_builder.mode, 0o666);
        assert_eq!(dir_builder.recursive, false);

        let dir_builder = DirBuilder::new().mode(0o777).recursive(true);
        assert_eq!(dir_builder.mode, 0o777);
        assert_eq!(dir_builder.recursive, true);
    }

    #[orengine_macros::test_local]
    fn test_dir_builder_create() {
        create_test_dir_if_not_exist();

        let dir_builder = DirBuilder::new().mode(0o777).recursive(false);
        let mut path = PathBuf::from(TEST_DIR_PATH);
        path.push("test_dir");
        match dir_builder.create(path.clone()).await {
            Ok(_) => assert!(is_exists(path)),
            Err(err) => panic!("Can't create dir: {}", err),
        }

        let dir_builder = DirBuilder::new().mode(0o777).recursive(true);
        let mut path = PathBuf::from(TEST_DIR_PATH);
        path.push("test_dir");
        path.push("test_dir2");
        path.push("test_dir3");
        path.push("test_dir4");
        path.push("test_dir5");
        match dir_builder.create(path.clone()).await {
            Ok(_) => assert!(is_exists(path)),
            Err(err) => panic!("Can't create dir all: {}", err),
        }

        let mut path = PathBuf::from(TEST_DIR_PATH);
        path.push("test_dir");
        std::fs::remove_dir_all(path.clone()).unwrap();
    }
}
