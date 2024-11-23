use io_uring::types::OpenHow;
use nix::libc::{self, c_int};
use std::fmt::Debug;
use std::io;
use std::io::Error;

/// Options and flags which can be used to configure how a file is opened.
#[derive(Copy, Clone)]
#[allow(clippy::struct_excessive_bools)] // false positive
pub struct OpenOptions {
    /// This option, when true, will indicate that the file should be read-able if opened.
    read: bool,
    /// This option, when true, will indicate that the file should be write-able if opened.
    ///
    /// If the file already exists,
    /// any write calls on it will overwrite its contents, without truncating it.
    write: bool,
    /// This option, when true, means that writes will append
    /// to a file instead of overwriting previous contents.
    ///
    /// Note that setting [`write(true)`](OpenOptions::write)[`append(true)`](OpenOptions::append)
    /// has the same effect as setting only [`append(true)`](OpenOptions::append).
    append: bool,
    /// If a file is successfully opened with this option set
    /// it will truncate the file to 0 length if it already exists.
    ///
    /// The file must be opened with write access for truncate to work.
    truncate: bool,
    /// This option, when true, will indicate that the file should be created if it does not exist.
    /// Else it will open the file.
    ///
    /// In order for the file to be created,
    /// [`OpenOptions::write`](OpenOptions::write)
    /// or [`OpenOptions::append`](OpenOptions::append) access must be used.
    create: bool,
    /// This option, when true, will indicate that the file should be created if it does not exist.
    /// Else it will return an [`error`](io::ErrorKind::AlreadyExists).
    ///
    /// For more information, see [`OpenOptions::create_new`](OpenOptions::create_new).
    create_new: bool,
    /// Pass custom flags to the flags argument of open.
    ///
    /// For more information, see [`OpenOptions::custom_flags`](OpenOptions::custom_flags).
    custom_flags: i32,
    /// The permissions to apply to the new file.
    mode: u32,
}

impl OpenOptions {
    /// Creates a blank new set of options ready for configuration.
    ///
    /// All options are initially set to false.
    pub const fn new() -> Self {
        Self {
            read: false,
            write: false,
            append: false,
            truncate: false,
            create: false,
            create_new: false,
            custom_flags: 0,
            mode: 0o666,
        }
    }

    /// Sets the option for read access.
    ///
    /// This option, when true, will indicate that the file should be read-able if opened.
    #[must_use]
    pub fn read(mut self, read: bool) -> Self {
        self.read = read;
        self
    }

    /// Sets the option for write access.
    ///
    /// This option, when true, will indicate that the file should be write-able if opened.
    ///
    /// If the file already exists, any write calls on it will overwrite its contents,
    /// without truncating it.
    #[must_use]
    pub fn write(mut self, write: bool) -> Self {
        self.write = write;
        self
    }

    /// Sets the option for the append mode.
    ///
    /// This option, when true, means that writes will append to a file
    /// instead of overwriting previous contents.
    /// Note that setting [`write(true)`](OpenOptions::write)[`append(true)`](OpenOptions::append)
    /// has the same effect as setting only [`append(true)`](OpenOptions::append).
    #[must_use]
    pub fn append(mut self, append: bool) -> Self {
        self.append = append;
        self
    }

    /// Sets the option for truncating a previous file.
    ///
    /// If a file is successfully opened with this option set it will truncate
    /// the file to 0 length if it already exists.
    ///
    /// The file must be opened with write access for truncate to work.
    #[must_use]
    pub fn truncate(mut self, truncate: bool) -> Self {
        self.truncate = truncate;
        self
    }

    /// Sets the option to create a new file, or open it if it already exists.
    /// In order for the file to be created,
    /// [`OpenOptions::write`] or [`OpenOptions::append`] access must be used.
    #[must_use]
    pub fn create(mut self, create: bool) -> Self {
        self.create = create;
        self
    }

    /// Sets the option to create a new file, failing if it already exists.
    ///
    /// If a file exists at the target location, creating a new file will fail
    /// with [`AlreadyExists`](io::ErrorKind::AlreadyExists) or another error based on the situation.
    /// See [`OpenOptions::open`](OpenOptions::open) for a non-exhaustive list of likely errors.
    ///
    /// This option is useful because it is atomic.
    /// Otherwise, between checking whether a file exists and creating a new one,
    /// the file may have been created by another process (a TOCTOU race condition / attack).
    ///
    /// If [`create_new(true)`](OpenOptions::create_new) is set, [`create()`](OpenOptions::create)
    /// and [`truncate()`](OpenOptions::truncate) are ignored.
    ///
    /// The file must be opened with write or append access in order to create a new file.
    #[must_use]
    pub fn create_new(mut self, create_new: bool) -> Self {
        self.create_new = create_new;
        self
    }

    /// Pass custom flags to the flags argument of open.
    ///
    /// The bits that define the access mode are masked out with `O_ACCMODE`,
    /// to ensure they do not interfere with the access mode set by Rusts options.
    ///
    /// Custom flags can only set flags, not remove flags set by Rusts options.
    /// This options overwrites any previously set custom flags.
    #[must_use]
    pub fn custom_flags(mut self, flags: i32) -> Self {
        self.custom_flags = flags;
        self
    }

    /// Sets the permissions to apply to the new file.
    #[must_use]
    pub fn mode(mut self, mode: u32) -> Self {
        self.mode = mode;
        self
    }

    #[cfg(unix)]
    /// Converts the `OpenOptions` into the argument to `open()` provided by the os.
    pub(crate) fn into_os_options(self) -> io::Result<OpenHow> {
        fn get_access_mode(opts: &OpenOptions) -> io::Result<c_int> {
            match (opts.read, opts.write, opts.append) {
                (true, false, false) => Ok(libc::O_RDONLY),
                (false, true, false) => Ok(libc::O_WRONLY),
                (true, true, false) => Ok(libc::O_RDWR),
                (false, _, true) => Ok(libc::O_WRONLY | libc::O_APPEND),
                (true, _, true) => Ok(libc::O_RDWR | libc::O_APPEND),
                (false, false, false) => Err(Error::from_raw_os_error(libc::EINVAL)),
            }
        }

        fn get_creation_mode(opts: &OpenOptions) -> io::Result<c_int> {
            match (opts.write, opts.append) {
                (true, false) => {}
                (false, false) => {
                    if opts.truncate || opts.create || opts.create_new {
                        return Err(Error::from_raw_os_error(libc::EINVAL));
                    }
                }
                (_, true) => {
                    if opts.truncate && !opts.create_new {
                        return Err(Error::from_raw_os_error(libc::EINVAL));
                    }
                }
            }

            Ok(match (opts.create, opts.truncate, opts.create_new) {
                (false, false, false) => 0,
                (true, false, false) => libc::O_CREAT,
                (false, true, false) => libc::O_TRUNC,
                (true, true, false) => libc::O_CREAT | libc::O_TRUNC,
                (_, _, true) => libc::O_CREAT | libc::O_EXCL,
            })
        }

        let flags = libc::O_CLOEXEC
            | get_access_mode(&self)?
            | get_creation_mode(&self)?
            | (self.custom_flags & !libc::O_ACCMODE);
        #[allow(clippy::cast_sign_loss)] // flags has no sign
        Ok(OpenHow::new().flags(flags as _).mode(self.mode.into()))
    }
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for OpenOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenOptions")
            .field("read", &self.read)
            .field("write", &self.write)
            .field("append", &self.append)
            .field("truncate", &self.truncate)
            .field("create", &self.create)
            .field("create_new", &self.create_new)
            .field("custom_flags", &self.custom_flags)
            .field("mode", &self.mode)
            .finish()
    }
}
