use crate::io::sys::OsOpenOptions;
use std::fmt::Debug;
use std::io;

/// Options and flags which can be used to configure how a file is opened.
#[derive(Copy, Clone)]
#[allow(clippy::struct_excessive_bools, reason = "False positive.")]
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
    #[cfg(unix)]
    mode: u32,
    #[cfg(windows)]
    access_mode: Option<u32>,
    #[cfg(windows)]
    share_mode: u32,
    #[cfg(windows)]
    attributes: u32,
    #[cfg(windows)]
    security_qos_flags: u32,
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
            #[cfg(unix)]
            mode: 0o666,
            #[cfg(windows)]
            access_mode: None,
            #[cfg(windows)]
            share_mode: 1u32 | 2u32 | 4u32,
            #[cfg(windows)]
            attributes: 0,
            #[cfg(windows)]
            security_qos_flags: 0,
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
    ///
    /// # Platform-specific behavior
    ///
    /// It has an effect only on unix-like platforms. In other platforms, it does nothing.
    #[must_use]
    #[allow(unused, reason = "We use #[cfg] here.")]
    pub fn mode(mut self, mode: u32) -> Self {
        #[cfg(unix)]
        {
            self.mode = mode;
        }

        self
    }

    /// Overrides the `dwDesiredAccess` argument to the call to [`CreateFile`]
    /// with the specified value.
    ///
    /// This will override the `read`, `write`, and `append` flags on the
    /// `OpenOptions` structure. This method provides fine-grained control over
    /// the permissions to read, write and append data, attributes (like hidden
    /// and system), and extended attributes.
    ///
    /// # Platform-specific behavior
    ///
    /// It has an effect only on windows. In other platforms, it does nothing.
    ///
    /// [`CreateFile`]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilea
    #[must_use]
    #[allow(unused, reason = "We use #[cfg] here.")]
    pub fn access_mode(mut self, access_mode: u32) -> Self {
        #[cfg(windows)]
        {
            self.access_mode = Some(access_mode);
        }

        self
    }

    /// Overrides the `dwShareMode` argument to the call to [`CreateFile`] with
    /// the specified value.
    ///
    /// By default, `share_mode` is set to
    /// `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE`. This allows
    /// other processes to read, write, and delete/rename the same file
    /// while it is open. Removing any of the flags will prevent other
    /// processes from performing the corresponding operation until the file
    /// handle is closed.
    ///
    /// # Platform-specific behavior
    ///
    /// It has an effect only on windows. In other platforms, it does nothing.
    ///
    /// [`CreateFile`]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilea
    #[must_use]
    #[allow(unused, reason = "We use #[cfg] here.")]
    pub fn share_mode(mut self, share_mode: u32) -> Self {
        #[cfg(windows)]
        {
            self.share_mode = share_mode;
        }

        self
    }
    /// Sets the `dwFileAttributes` argument to the call to [`CreateFile2`] to
    /// the specified value (or combines it with `custom_flags` and
    /// `security_qos_flags` to set the `dwFlagsAndAttributes` for
    /// [`CreateFile`]).
    ///
    /// If a _new_ file is created because it does not yet exist and
    /// `.create(true)` or `.create_new(true)` are specified, the new file is
    /// given the attributes declared with `.attributes()`.
    ///
    /// If an _existing_ file is opened with `.create(true).truncate(true)`, its
    /// existing attributes are preserved and combined with the ones declared
    /// with `.attributes()`.
    ///
    /// In all other cases the attributes get ignored.
    ///
    /// # Platform-specific behavior
    ///
    /// It has an effect only on windows. In other platforms, it does nothing.
    ///
    /// [`CreateFile`]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilea
    /// [`CreateFile2`]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfile2
    #[must_use]
    #[allow(unused, reason = "We use #[cfg] here.")]
    pub fn attributes(mut self, attrs: u32) -> Self {
        #[cfg(windows)]
        {
            self.attributes = attrs;
        }

        self
    }

    #[cfg(target_os = "linux")]
    /// Converts the `OpenOptions` into the argument to `open()` provided by the os.
    pub(crate) fn into_os_options(mut self) -> io::Result<OsOpenOptions> {
        use libc;

        let access_mode = match (self.read, self.write, self.append) {
            (true, false, false) => libc::O_RDONLY,
            (false, true, false) => libc::O_WRONLY,
            (true, true, false) => libc::O_RDWR,
            (false, _, true) => libc::O_WRONLY | libc::O_APPEND,
            (true, _, true) => libc::O_RDWR | libc::O_APPEND,
            (false, false, false) => return Err(io::Error::from_raw_os_error(libc::EINVAL)),
        };

        let creation_flags = match (self.create, self.truncate, self.create_new) {
            (false, false, false) => {
                self.mode = 0;
                0
            }
            (false, true, false) => {
                self.mode = 0;
                libc::O_TRUNC
            }
            (true, false, false) => libc::O_CREAT,
            (true, true, false) => libc::O_CREAT | libc::O_TRUNC,
            (_, _, true) => libc::O_CREAT | libc::O_EXCL,
        };

        #[allow(clippy::cast_sign_loss, reason = "Flags don't have signs.")]
        Ok(OsOpenOptions::new()
            .flags(
                (libc::O_CLOEXEC
                    | access_mode
                    | creation_flags
                    | (self.custom_flags & !libc::O_ACCMODE)) as u64,
            )
            .mode(self.mode.into()))
    }

    #[cfg(not(target_os = "linux"))]
    /// Converts the `OpenOptions` into the argument to `open()` provided by the os.
    pub(crate) fn into_os_options(self) -> io::Result<OsOpenOptions> {
        Ok(std::fs::OpenOptions::from(self))
    }
}

impl From<OpenOptions> for std::fs::OpenOptions {
    fn from(options: OpenOptions) -> Self {
        let mut open_options = Self::new();

        open_options.read(options.read);
        open_options.write(options.write);
        open_options.append(options.append);
        open_options.truncate(options.truncate);
        open_options.create(options.create);
        open_options.create_new(options.create_new);

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;

            open_options.mode(options.mode);
            open_options.custom_flags(options.custom_flags);
        }

        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;

            open_options.custom_flags(options.custom_flags as u32);

            if let Some(access_mode) = options.access_mode {
                open_options.access_mode(access_mode);
            }

            open_options.share_mode(options.share_mode);
            open_options.attributes(options.attributes);
            open_options.security_qos_flags(options.security_qos_flags);
        }

        open_options
    }
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for OpenOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug_struct = f.debug_struct("OpenOptions");

        debug_struct
            .field("read", &self.read)
            .field("write", &self.write)
            .field("append", &self.append)
            .field("truncate", &self.truncate)
            .field("create", &self.create)
            .field("create_new", &self.create_new)
            .field("custom_flags", &self.custom_flags);

        #[cfg(unix)]
        {
            debug_struct.field("mode", &self.mode);
        }

        #[cfg(windows)]
        {
            debug_struct
                .field("access_mode", &self.access_mode)
                .field("share_mode", &self.share_mode)
                .field("security_qos_flags", &self.security_qos_flags)
                .field("attributes", &self.attributes);
        }

        debug_struct.finish()
    }
}
