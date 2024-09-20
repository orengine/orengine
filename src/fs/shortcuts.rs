use std::io::{Result};
use std::path::Path;
use crate::fs::{DirBuilder, File, OpenOptions};
use crate::io::remove_dir::RemoveDir;
use crate::io::sys::OsPath::get_os_path;

/// Opens a file asynchronously with the specified open options.
///
/// This function is a wrapper around the [`File::open`] method, which opens a file
/// located at `path` with the provided `open_options`.
///
/// # Example
///
/// ```no_run
/// use orengine::fs::{File, OpenOptions, open_file};
///
/// # async fn foo() -> std::io::Result<()> {
/// let options = OpenOptions::new().read(true).write(true);
/// let file = open_file("example.txt", &options).await?;
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// This function will return an `Err` if the file cannot be opened due to I/O errors
/// (e.g., file not found, permission denied).
#[inline(always)]
pub async fn open_file<P: AsRef<Path>>(path: P, open_options: &OpenOptions) -> Result<File> {
    File::open(path, open_options).await
}

/// Creates a new directory at the specified path.
///
/// This function asynchronously creates a single directory at `path`
/// unlike [`create_dir_all`](create_dir_all) which creates a full directory tree.
///
/// If the directory already exists, the function does nothing.
///
/// # Example
///
/// ```no_run
/// use orengine::fs::create_dir;
///
/// # async fn foo() -> std::io::Result<()> {
/// create_dir("new_directory").await?;
/// assert!(std::fs::exists("new_directory")?);
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// This function will return an `Err` if the directory cannot be created due to I/O errors
/// (e.g., permission denied, path does not exist).
#[inline(always)]
pub async fn create_dir<P: AsRef<Path>>(path: P) -> Result<()> {
    DirBuilder::new().create(path).await
}

/// Recursively creates all directories in the given path.
///
/// This function creates the entire directory tree if it doesn't exist,
/// unlike [`create_dir`](create_dir) which only creates a single directory.
///
/// # Example
///
/// ```no_run
/// use orengine::fs::create_dir_all;
///
/// # async fn foo() -> std::io::Result<()> {
/// create_dir_all("parent/child/grandchild").await?;
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// This function will return an `Err` if any directory in the path cannot be created
/// due to I/O errors.
#[inline(always)]
pub async fn create_dir_all<P: AsRef<Path>>(path: P) -> Result<()> {
    DirBuilder::new().recursive(true).create(path).await
}

/// Removes the directory at the specified path.
///
/// This function asynchronously deletes a directory. The directory must be empty,
/// otherwise an error will be returned.
///
/// # Example
///
/// ```no_run
/// use orengine::fs::remove_dir;
///
/// # async fn foo() -> std::io::Result<()> {
/// remove_dir("empty_directory").await?;
/// assert!(!std::fs::exists("empty_directory")?);
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// This function will return an `Err` if the directory is not empty or cannot be deleted
/// due to I/O errors.
#[inline(always)]
pub async fn remove_dir<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = get_os_path(path.as_ref())?;
    RemoveDir::new(path).await
}

/// Removes the file at the specified path.
///
/// This function asynchronously deletes a file at `path`. If the file does not exist,
/// an error will be returned.
///
/// # Example
///
/// ```no_run
/// use orengine::fs::remove_file;
///
/// # async fn foo() -> std::io::Result<()> {
/// remove_file("example.txt").await?;
/// assert!(!std::fs::exists("example.txt")?);
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// This function will return an `Err` if the file cannot be deleted due to I/O errors
/// (e.g., permission denied, file not found).
#[inline(always)]
pub async fn remove_file<P: AsRef<Path>>(path: P) -> Result<()> {
    File::remove(path).await
}

/// Renames a file or directory from one path to another.
///
/// This function asynchronously renames `old_path` to `new_path`. Both paths must refer
/// to valid file or directory names.
///
/// # Example
///
/// ```no_run
/// use orengine::fs::rename;
///
/// # async fn foo() -> std::io::Result<()> {
/// rename("old_name.txt", "new_name.txt").await?;
/// assert!(!std::fs::exists("old_name.txt")?);
/// assert!(std::fs::exists("new_name.txt")?);
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// This function will return an `Err` if the rename operation fails due to I/O errors
/// (e.g., permission denied, file not found).
#[inline(always)]
pub async fn rename<OldPath, NewPath>(old_path: OldPath, new_path: NewPath) -> Result<()>
where
    OldPath: AsRef<Path>,
    NewPath: AsRef<Path>
{
    File::rename(old_path, new_path).await
}

#[cfg(test)]
/// we need to check only [`remove_dir`], because all others functions was already tested in
/// [`file`](crate::fs::file) or [`dir_builder`](crate::fs::dir_builder).
mod tests {
    use std::path::PathBuf;
    use super::*;
    use crate::fs::test_helper::{create_test_dir_if_not_exist, is_exists, TEST_DIR_PATH};

    #[orengine_macros::test]
    fn test_remove_dir() {
        create_test_dir_if_not_exist();

        let mut path = PathBuf::from(TEST_DIR_PATH);
        path.push("remove_dir");
        match create_dir(path.clone()).await {
            Ok(_) => assert!(is_exists(path.clone())),
            Err(err) => panic!("Can't create dir: {}", err)
        }

        match remove_dir(path.clone()).await {
            Ok(_) => assert!(!is_exists(path)),
            Err(err) => panic!("Can't remove dir: {}", err)
        }
    }
}