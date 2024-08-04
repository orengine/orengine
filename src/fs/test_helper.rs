use std::ffi::OsStr;
use std::os::unix::fs::DirBuilderExt;
use std::path::Path;

#[cfg(test)]
pub(crate) const TEST_DIR_PATH: &str = "./test";

/// Returns true if the given path exists.
#[cfg(test)]
pub(crate) fn is_exists<S: AsRef<OsStr>>(path: S) -> bool {
   Path::new(path.as_ref()).exists()
}


#[cfg(test)]
pub(crate) fn create_test_dir_if_not_exist() {
    if !is_exists(TEST_DIR_PATH) {
        // here we use std, because we are in tests, so we can't be sure that crate::fs::create_dir_all works correctly
        std::fs::DirBuilder::new().mode(0o777).recursive(true).create(TEST_DIR_PATH).unwrap();
    }
}

#[cfg(test)]
pub(crate) fn create_file_if_not_exists(path: &str) {
    if !is_exists(path) {
        match std::fs::File::create(path) {
            Ok(_) => { return; }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => { panic!("{}", e) },
            _ => {}
        }

        let dir = Path::new(path).parent().unwrap();
        std::fs::DirBuilder::new().mode(0o777).recursive(true).create(dir).unwrap();
        std::fs::File::create(path).unwrap();

        std::fs::File::create(path).expect("create file failed");
    }
}

#[cfg(test)]
pub(crate) fn delete_file_if_exists(path: &str) {
    if is_exists(path) {
        std::fs::remove_file(path).unwrap();
    }
}