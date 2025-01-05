use std::ffi::OsStr;
#[cfg(unix)]
use std::os::unix::fs::DirBuilderExt;
use std::path::Path;

/// The path of the test directory.
#[cfg(test)]
pub(crate) const TEST_DIR_PATH: &str = "./test";

/// Returns true if the given path exists.
#[cfg(test)]
pub(crate) fn is_exists<S: AsRef<OsStr>>(path: S) -> bool {
    Path::new(path.as_ref()).exists()
}

/// Creates the test directory if it does not exist.
#[cfg(test)]
pub(crate) fn create_test_dir_if_not_exist() {
    if !is_exists(TEST_DIR_PATH) {
        // here we use std, because we are in tests, so we can't be sure that crate::fs::create_dir_all works correctly
        let mut dir_builder = std::fs::DirBuilder::new();
        let dir_builder_ref = dir_builder.recursive(true);
        #[cfg(unix)]
        let dir_builder_ref = dir_builder_ref.mode(0o777);
        dir_builder_ref
            .create(TEST_DIR_PATH)
            .expect("can't create test dir");
    }
}
