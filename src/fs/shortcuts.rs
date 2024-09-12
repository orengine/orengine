use std::io::{Result};
use std::path::Path;
use crate::fs::{DirBuilder, File, OpenOptions};
use crate::io::remove_dir::RemoveDir;
use crate::io::sys::OsPath::get_os_path;

#[inline(always)]
pub async fn open_file<P: AsRef<Path>>(path: P, open_options: &OpenOptions) -> Result<File> {
    File::open(path, open_options).await
}

#[inline(always)]
pub async fn create_dir<P: AsRef<Path>>(path: P) -> Result<()> {
    DirBuilder::new().create(path).await
}

#[inline(always)]
pub async fn create_dir_all<P: AsRef<Path>>(path: P) -> Result<()> {
    DirBuilder::new().recursive(true).create(path).await
}

#[inline(always)]
pub async fn remove_dir<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = get_os_path(path.as_ref())?;
    RemoveDir::new(path).await
}

#[inline(always)]
pub async fn remove_file<P: AsRef<Path>>(path: P) -> Result<()> {
    File::remove(path).await
}

#[inline(always)]
pub async fn rename<OldPath, NewPath>(old_path: OldPath, new_path: NewPath) -> Result<()>
where
    OldPath: AsRef<Path>,
    NewPath: AsRef<Path>
{
    File::rename(old_path, new_path).await
}

// TODO create a docs for using [`std::fs::read_dir`] and [`std::fs::remove_dir_all`].
// We can do it with creating new os_thread that will write to the channel after the operation.

#[cfg(test)]
/// we need to check only [`remove_dir`], because all others functions was already tested in
/// [`file`](crate::fs::file) or [`dir_builder`](crate::fs::dir_builder).
mod tests {
    use std::path::PathBuf;
    use super::*;
    use crate::fs::test_helper::{create_test_dir_if_not_exist, is_exists, TEST_DIR_PATH};

    #[test_macro::test]
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