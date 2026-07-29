//! Store-root capability for project records.
//!
//! Product code never reopens an aggregate record through an absolute path.
//! The store root and every descendant directory are retained by handle; all
//! Unix reads, writes, listings, and deletes are descriptor-relative and use
//! `O_NOFOLLOW`. Windows rejects reparse points and verifies directory/file
//! handle identity before and after every pathname publication. Platforms
//! without either backend fail closed.

use crate::{Result, ScienceError};
#[cfg(unix)]
use std::ffi::OsStr;
use std::{
    ffi::OsString,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

/// Fixed lock record used to serialise every project-store mutation.
///
/// The name is deliberately not derived from a pathname or project id: one
/// retained store-root capability has exactly one writer domain.
#[cfg(unix)]
pub(crate) const PROJECT_WRITE_LOCK_FILE: &str = ".lumen-project-write.lock";

/// Held proof that the project-store writer domain is locked.
///
/// Unix retains the locked file description for the lifetime of this value.
/// Other backends currently provide process-only serialisation in
/// `ProjectStore`; the empty marker is intentionally not described as a
/// cross-process lock.
pub(crate) struct ProjectWriteFileLock {
    #[cfg(unix)]
    _file: fs::File,
    #[cfg(windows)]
    _file: fs::File,
    #[cfg(not(any(unix, windows)))]
    _process_only: (),
}

fn validate_relative(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(ScienceError::Invalid(
            "project record path must be non-empty and relative".into(),
        ));
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ScienceError::Invalid(
            "project record path contains a non-normal component".into(),
        ));
    }
    Ok(())
}

#[cfg(unix)]
#[derive(Debug)]
pub(crate) struct PinnedDirectory {
    file: fs::File,
}

#[cfg(unix)]
impl PinnedDirectory {
    pub(crate) fn open_or_create(path: &Path) -> Result<Self> {
        // The ACP product path securely provisions the store root before it
        // constructs ProjectStore. Creating an absent root here is retained
        // for embedders/tests, but is not itself a defence against a hostile
        // pre-existing ancestor; confinement begins once the canonical root
        // descriptor below is retained.
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ScienceError::Invalid(
                    "project store root must not be a symlink".into(),
                ));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(ScienceError::Invalid(
                    "project store root must be a directory".into(),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir_all(path)?;
            }
            Err(error) => return Err(error.into()),
        }
        Self::open_existing(path)
    }

    fn open_existing(path: &Path) -> Result<Self> {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ScienceError::Invalid(
                    "project store root must not be a symlink".into(),
                ));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(ScienceError::Invalid(
                    "project store root must be a directory".into(),
                ));
            }
            Ok(_) => {}
            Err(error) => return Err(error.into()),
        }
        let canonical = dunce::canonicalize(path)?;
        if !canonical.is_absolute() {
            return Err(ScienceError::Invalid(
                "project store root must resolve to an absolute path".into(),
            ));
        }
        let mut options = fs::OpenOptions::new();
        use std::os::unix::fs::OpenOptionsExt as _;
        options.read(true).custom_flags(
            libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        );
        let mut current = Self {
            file: options.open(Path::new("/"))?,
        };
        for component in canonical.components() {
            match component {
                Component::RootDir | Component::CurDir => {}
                Component::Normal(name) => current = current.open_child(name)?,
                _ => {
                    return Err(ScienceError::Invalid(
                        "project store root contains an unsupported component".into(),
                    ));
                }
            }
        }
        Ok(current)
    }

    /// Retain an already-provisioned directory below `workspace`.
    ///
    /// Unlike [`Self::open_or_create_within`], this never creates the root.
    /// Approval-gated callers use it during prepare so a denied request cannot
    /// leave a directory behind merely by asking for permission.
    pub(crate) fn open_existing_within(path: &Path, workspace: &Path) -> Result<Self> {
        use std::os::unix::fs::MetadataExt as _;

        let canonical_workspace = dunce::canonicalize(workspace)?;
        if !canonical_workspace.is_absolute() {
            return Err(ScienceError::Invalid(
                "workspace must resolve to an absolute path".into(),
            ));
        }
        let opened = Self::open_existing(path)?;
        let canonical_path = dunce::canonicalize(path)?;
        if !canonical_path.starts_with(&canonical_workspace) {
            return Err(ScienceError::Invalid(
                "project store root escapes the canonical workspace".into(),
            ));
        }
        let reopened = Self::open_existing(&canonical_path)?;
        let opened_metadata = opened.file.metadata()?;
        let reopened_metadata = reopened.file.metadata()?;
        if opened_metadata.dev() != reopened_metadata.dev()
            || opened_metadata.ino() != reopened_metadata.ino()
        {
            return Err(ScienceError::Invalid(
                "project store root identity changed during confinement".into(),
            ));
        }
        Ok(opened)
    }

    /// Open `path` once, retain that exact directory, and prove that the
    /// retained directory is the same object as a canonical path below
    /// `workspace`. The identity comparison closes the canonicalize -> open
    /// swap window; later I/O remains relative to the retained descriptor.
    pub(crate) fn open_or_create_within(path: &Path, workspace: &Path) -> Result<Self> {
        use std::os::unix::fs::MetadataExt as _;

        let canonical_workspace = dunce::canonicalize(workspace)?;
        if !canonical_workspace.is_absolute() {
            return Err(ScienceError::Invalid(
                "workspace must resolve to an absolute path".into(),
            ));
        }
        let opened = Self::open_or_create(path)?;
        let canonical_path = dunce::canonicalize(path)?;
        if !canonical_path.starts_with(&canonical_workspace) {
            return Err(ScienceError::Invalid(
                "project store root escapes the canonical workspace".into(),
            ));
        }
        let reopened = Self::open_or_create(&canonical_path)?;
        let opened_metadata = opened.file.metadata()?;
        let reopened_metadata = reopened.file.metadata()?;
        if opened_metadata.dev() != reopened_metadata.dev()
            || opened_metadata.ino() != reopened_metadata.ino()
        {
            return Err(ScienceError::Invalid(
                "project store root identity changed during confinement".into(),
            ));
        }
        Ok(opened)
    }

    /// Block until this exact retained store root owns its cross-process
    /// writer lock.
    ///
    /// The root and lock record are validated through retained descriptors.
    /// A pathname symlink, non-private lock record, hard link, foreign owner,
    /// or inode swap fails closed before a caller can mutate project state.
    pub(crate) fn lock_project_writes(&self) -> Result<ProjectWriteFileLock> {
        use std::os::{
            fd::AsRawFd as _,
            unix::fs::{MetadataExt as _, PermissionsExt as _},
        };

        let root_metadata = self.file.metadata()?;
        // The retained root need not be unreadable (0755 is acceptable), but
        // another uid/group must not be able to replace the fixed lock name.
        if !root_metadata.is_dir()
            || root_metadata.uid() != effective_user_id()
            || root_metadata.permissions().mode() & 0o022 != 0
        {
            return Err(ScienceError::Invalid(
                "project store root must be owner-controlled and not group/world writable".into(),
            ));
        }

        let lock_name = OsStr::new(PROJECT_WRITE_LOCK_FILE);
        let file = openat(
            &self.file,
            lock_name,
            libc::O_RDWR | libc::O_CREAT | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            Some(0o600),
        )
        .map_err(|error| match error {
            ScienceError::Io(io)
                if io.raw_os_error() == Some(libc::ELOOP)
                    || io.raw_os_error() == Some(libc::EISDIR) =>
            {
                ScienceError::Invalid(
                    "project store write lock must be a private regular file".into(),
                )
            }
            error => error,
        })?;
        validate_project_write_lock_file(&file)?;

        loop {
            // SAFETY: `file` owns a live descriptor and remains retained by
            // the returned guard after flock succeeds.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0 {
                break;
            }
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error.into());
        }

        // Reopen the fixed name *after* locking and compare identities. This
        // detects replacement between the initial open and acquisition.
        let reopened = openat(
            &self.file,
            lock_name,
            libc::O_RDWR | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            None,
        )
        .map_err(|error| match error {
            ScienceError::Io(io) if io.raw_os_error() == Some(libc::ELOOP) => {
                ScienceError::Invalid("project store write lock must not be a symlink".into())
            }
            error => error,
        })?;
        validate_project_write_lock_file(&reopened)?;
        let locked_metadata = file.metadata()?;
        let reopened_metadata = reopened.metadata()?;
        if locked_metadata.dev() != reopened_metadata.dev()
            || locked_metadata.ino() != reopened_metadata.ino()
        {
            return Err(ScienceError::Invalid(
                "project store write lock identity changed during acquisition".into(),
            ));
        }

        Ok(ProjectWriteFileLock { _file: file })
    }

    pub(crate) fn read_optional(&self, relative: &Path) -> Result<Option<Vec<u8>>> {
        validate_relative(relative)?;
        let parent = match self.open_parent(relative, false) {
            Ok(parent) => parent,
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let name = relative
            .file_name()
            .ok_or_else(|| ScienceError::Invalid("project record has no file name".into()))?;
        let mut file = match openat(
            &parent.file,
            name,
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            None,
        ) {
            Ok(file) => file,
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(ScienceError::Io(error)) if error.raw_os_error() == Some(libc::ELOOP) => {
                return Err(ScienceError::Invalid(
                    "project record must not be a symlink".into(),
                ));
            }
            Err(error) => return Err(error),
        };
        if !file.metadata()?.is_file() {
            return Err(ScienceError::Invalid(
                "project record must be a regular file".into(),
            ));
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(Some(bytes))
    }

    pub(crate) fn replace_atomic(&self, relative: &Path, bytes: &[u8]) -> Result<()> {
        self.publish(relative, bytes, false)
    }

    pub(crate) fn write_new_atomic(&self, relative: &Path, bytes: &[u8]) -> Result<()> {
        self.publish(relative, bytes, true)
    }

    fn publish(&self, relative: &Path, bytes: &[u8], create_only: bool) -> Result<()> {
        validate_relative(relative)?;
        let parent = self.open_parent(relative, true)?;
        let target = relative
            .file_name()
            .ok_or_else(|| ScienceError::Invalid("project record has no file name".into()))?;
        let temp = OsString::from(format!(".project-{}.tmp", Uuid::new_v4()));
        let mut staged = openat(
            &parent.file,
            &temp,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            Some(0o600),
        )?;
        let result = (|| -> Result<()> {
            staged.write_all(bytes)?;
            staged.sync_all()?;
            if create_only {
                linkat(&parent.file, &temp, target)?;
                unlinkat(&parent.file, &temp, 0)?;
            } else {
                renameat(&parent.file, &temp, target)?;
            }
            parent.file.sync_all()?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = unlinkat(&parent.file, &temp, 0);
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn list_names(&self, relative: &Path) -> Result<Vec<OsString>> {
        let directory = match self.open_directory(relative) {
            Ok(directory) => directory,
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Vec::new());
            }
            Err(error) => return Err(error),
        };
        readdir_names(&directory.file)
    }

    pub(crate) fn remove_file(&self, relative: &Path) -> Result<bool> {
        validate_relative(relative)?;
        let parent = match self.open_parent(relative, false) {
            Ok(parent) => parent,
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        let name = relative
            .file_name()
            .ok_or_else(|| ScienceError::Invalid("project record has no file name".into()))?;
        match unlinkat(&parent.file, name, 0) {
            Ok(()) => {
                parent.file.sync_all()?;
                Ok(true)
            }
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    /// Create descendants relative to this retained directory and return a
    /// handle to the final directory. No later operation needs to reopen the
    /// original absolute path.
    pub(crate) fn create_directory(&self, relative: &Path) -> Result<Self> {
        self.create_directories(relative)
    }

    /// Duplicate this directory for inheritance by an approved child process.
    ///
    /// `F_DUPFD` returns a descriptor with `FD_CLOEXEC` cleared. The caller
    /// must retain the returned `File` until the child has exited.
    pub(crate) fn duplicate_inheritable(&self) -> Result<fs::File> {
        use std::os::fd::{AsRawFd as _, FromRawFd as _};

        // SAFETY: `self.file` owns a live descriptor. F_DUPFD creates a new
        // owned descriptor at or above 3 and leaves FD_CLOEXEC clear.
        let duplicated = unsafe { libc::fcntl(self.file.as_raw_fd(), libc::F_DUPFD, 3) };
        if duplicated < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: a successful F_DUPFD transfers one independent descriptor.
        Ok(unsafe { fs::File::from_raw_fd(duplicated) })
    }

    /// Recursively read regular files below this retained directory.
    ///
    /// Every entry is opened relative to a live directory descriptor with
    /// `O_NOFOLLOW`. A symlink or any non-file/non-directory entry rejects the
    /// entire snapshot; it is never skipped and never followed.
    /// Recursively read regular files without ever buffering more than
    /// `max_bytes` in aggregate.
    pub(crate) fn snapshot_nofollow_bounded(
        &self,
        max_bytes: u64,
    ) -> Result<Vec<(PathBuf, Vec<u8>)>> {
        let mut files = Vec::new();
        let mut bytes_read = 0u64;
        self.snapshot_nofollow_into(Path::new(""), &mut files, &mut bytes_read, max_bytes)?;
        Ok(files)
    }

    fn snapshot_nofollow_into(
        &self,
        prefix: &Path,
        files: &mut Vec<(PathBuf, Vec<u8>)>,
        bytes_read: &mut u64,
        max_bytes: u64,
    ) -> Result<()> {
        for name in readdir_names(&self.file)? {
            let relative = prefix.join(&name);
            let opened = openat(
                &self.file,
                &name,
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                None,
            )
            .map_err(|error| match error {
                ScienceError::Io(io) if io.raw_os_error() == Some(libc::ELOOP) => {
                    ScienceError::Invalid(format!(
                        "workflow output snapshot refuses symlink '{}'",
                        relative.display()
                    ))
                }
                error => error,
            })?;
            let metadata = opened.metadata()?;
            if metadata.is_dir() {
                Self { file: opened }
                    .snapshot_nofollow_into(&relative, files, bytes_read, max_bytes)?;
            } else if metadata.is_file() {
                let remaining = max_bytes.saturating_sub(*bytes_read);
                let mut bytes = Vec::new();
                opened
                    .take(remaining.saturating_add(1))
                    .read_to_end(&mut bytes)?;
                *bytes_read = bytes_read.checked_add(bytes.len() as u64).ok_or_else(|| {
                    ScienceError::Invalid("workflow output snapshot byte count overflowed".into())
                })?;
                if *bytes_read > max_bytes {
                    return Err(ScienceError::Invalid(format!(
                        "workflow output exceeds the admitted {max_bytes} byte cap"
                    )));
                }
                files.push((relative, bytes));
            } else {
                return Err(ScienceError::Invalid(format!(
                    "workflow output snapshot refuses special entry '{}'",
                    relative.display()
                )));
            }
        }
        Ok(())
    }

    fn open_parent(&self, relative: &Path, create: bool) -> Result<Self> {
        validate_relative(relative)?;
        match relative.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => {
                if create {
                    self.create_directories(parent)
                } else {
                    self.open_directory(parent)
                }
            }
            _ => self.try_clone(),
        }
    }

    fn open_directory(&self, relative: &Path) -> Result<Self> {
        if relative.as_os_str().is_empty() {
            return self.try_clone();
        }
        validate_relative(relative)?;
        let mut current = self.try_clone()?;
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err(ScienceError::Invalid(
                    "project directory contains a non-normal component".into(),
                ));
            };
            current = current.open_child(name)?;
        }
        Ok(current)
    }

    fn create_directories(&self, relative: &Path) -> Result<Self> {
        if relative.as_os_str().is_empty() {
            return self.try_clone();
        }
        validate_relative(relative)?;
        let mut current = self.try_clone()?;
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err(ScienceError::Invalid(
                    "project directory contains a non-normal component".into(),
                ));
            };
            match mkdirat(&current.file, name, 0o700) {
                Ok(()) => {}
                Err(ScienceError::Io(error))
                    if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
            current = current.open_child(name)?;
        }
        Ok(current)
    }

    fn open_child(&self, name: &OsStr) -> Result<Self> {
        let file = openat(
            &self.file,
            name,
            libc::O_RDONLY
                | libc::O_DIRECTORY
                | libc::O_CLOEXEC
                | libc::O_NOFOLLOW
                | libc::O_NONBLOCK,
            None,
        )
        .map_err(|error| match error {
            ScienceError::Io(io)
                if io.raw_os_error() == Some(libc::ELOOP)
                    || io.raw_os_error() == Some(libc::ENOTDIR) =>
            {
                ScienceError::Invalid("project directory component must not be a symlink".into())
            }
            error => error,
        })?;
        if !file.metadata()?.is_dir() {
            return Err(ScienceError::Invalid(
                "project path component is not a directory".into(),
            ));
        }
        Ok(Self { file })
    }

    fn try_clone(&self) -> Result<Self> {
        Ok(Self {
            file: self.file.try_clone()?,
        })
    }

    pub(crate) fn identity(&self) -> Result<crate::StoreRootIdentity> {
        use std::os::unix::fs::MetadataExt as _;

        let metadata = self.file.metadata()?;
        Ok(crate::StoreRootIdentity::Unix {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}

#[cfg(unix)]
fn validate_project_write_lock_file(file: &fs::File) -> Result<()> {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != effective_user_id()
        || metadata.permissions().mode() & 0o777 != 0o600
        || metadata.nlink() != 1
    {
        return Err(ScienceError::Invalid(
            "project store write lock must be an owner-owned 0600 regular file with one link"
                .into(),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn effective_user_id() -> libc::uid_t {
    // SAFETY: geteuid has no preconditions and reads process credentials.
    unsafe { libc::geteuid() }
}

#[cfg(unix)]
fn os_name(name: &OsStr) -> Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt as _;
    std::ffi::CString::new(name.as_bytes())
        .map_err(|_| ScienceError::Invalid("path component contains NUL".into()))
}

#[cfg(unix)]
fn openat(
    directory: &fs::File,
    name: &OsStr,
    flags: i32,
    mode: Option<libc::mode_t>,
) -> Result<fs::File> {
    use std::os::fd::{AsRawFd as _, FromRawFd as _};
    let name = os_name(name)?;
    // SAFETY: the directory descriptor and NUL-terminated name are live.
    let fd = unsafe {
        match mode {
            Some(mode) => libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                flags,
                libc::c_uint::from(mode),
            ),
            None => libc::openat(directory.as_raw_fd(), name.as_ptr(), flags),
        }
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: a successful openat transfers one owned descriptor.
    Ok(unsafe { fs::File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn mkdirat(directory: &fs::File, name: &OsStr, mode: libc::mode_t) -> Result<()> {
    use std::os::fd::AsRawFd as _;
    let name = os_name(name)?;
    // SAFETY: the directory descriptor and NUL-terminated name are live.
    if unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), mode) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

#[cfg(unix)]
fn linkat(directory: &fs::File, source: &OsStr, target: &OsStr) -> Result<()> {
    use std::os::fd::AsRawFd as _;
    let source = os_name(source)?;
    let target = os_name(target)?;
    // SAFETY: both names are relative to the same retained directory.
    if unsafe {
        libc::linkat(
            directory.as_raw_fd(),
            source.as_ptr(),
            directory.as_raw_fd(),
            target.as_ptr(),
            0,
        )
    } == 0
    {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

#[cfg(unix)]
fn renameat(directory: &fs::File, source: &OsStr, target: &OsStr) -> Result<()> {
    use std::os::fd::AsRawFd as _;
    let source = os_name(source)?;
    let target = os_name(target)?;
    // SAFETY: both names are relative to the same retained directory.
    if unsafe {
        libc::renameat(
            directory.as_raw_fd(),
            source.as_ptr(),
            directory.as_raw_fd(),
            target.as_ptr(),
        )
    } == 0
    {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

#[cfg(unix)]
fn unlinkat(directory: &fs::File, name: &OsStr, flags: i32) -> Result<()> {
    use std::os::fd::AsRawFd as _;
    let name = os_name(name)?;
    // SAFETY: the name is relative to the retained directory descriptor.
    if unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), flags) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

#[cfg(unix)]
fn readdir_names(directory: &fs::File) -> Result<Vec<OsString>> {
    use std::os::{fd::AsRawFd as _, unix::ffi::OsStringExt as _};
    // `dup` would share the directory offset with the retained capability, so
    // a second snapshot could silently observe an empty directory. Reopening
    // "." relative to the live descriptor creates a fresh open-file
    // description while remaining anchored to the exact retained directory.
    // SAFETY: the relative name is a static NUL-terminated C string.
    let duplicated = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            c".".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if duplicated < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: fdopendir takes ownership of the duplicate.
    let stream = unsafe { libc::fdopendir(duplicated) };
    if stream.is_null() {
        // SAFETY: fdopendir failed and did not take ownership.
        unsafe { libc::close(duplicated) };
        return Err(std::io::Error::last_os_error().into());
    }
    let mut names = Vec::new();
    loop {
        // SAFETY: stream remains live until closed below.
        let entry = unsafe { libc::readdir(stream) };
        if entry.is_null() {
            break;
        }
        // SAFETY: d_name is NUL-terminated by readdir.
        let raw = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
        if raw != b"." && raw != b".." {
            names.push(OsString::from_vec(raw.to_vec()));
        }
    }
    // SAFETY: close exactly once; it also closes the duplicated descriptor.
    if unsafe { libc::closedir(stream) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    names.sort();
    Ok(names)
}

#[cfg(windows)]
#[derive(Debug)]
pub(crate) struct PinnedDirectory {
    path: PathBuf,
    file: fs::File,
}

#[cfg(windows)]
impl PinnedDirectory {
    pub(crate) fn open_or_create(path: &Path) -> Result<Self> {
        // The ACP product path securely provisions this root first. The
        // fallback creation keeps standalone embedders functional; subsequent
        // I/O is accepted only while the retained handle identity still
        // matches the non-reparse path.
        match fs::symlink_metadata(path) {
            Ok(metadata) if windows_has_reparse_point(&metadata) => {
                return Err(ScienceError::Invalid(
                    "project store root must not be a Windows reparse point".into(),
                ));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(ScienceError::Invalid(
                    "project store root must be a directory".into(),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir_all(path)?;
            }
            Err(error) => return Err(error.into()),
        }
        Self::open_existing(path)
    }

    fn open_existing(path: &Path) -> Result<Self> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        windows_assert_no_reparse_components(&absolute)?;
        Self::open_verified_directory(dunce::canonicalize(absolute)?)
    }

    pub(crate) fn open_existing_within(path: &Path, workspace: &Path) -> Result<Self> {
        let canonical_workspace = dunce::canonicalize(workspace)?;
        let opened = Self::open_existing(path)?;
        let canonical_path = dunce::canonicalize(path)?;
        if !canonical_path.starts_with(&canonical_workspace) {
            return Err(ScienceError::Invalid(
                "project store root escapes the canonical workspace".into(),
            ));
        }
        let reopened = Self::open_existing(&canonical_path)?;
        if !windows_same_open_file(&opened.file, &reopened.file) {
            return Err(ScienceError::Invalid(
                "project store root identity changed during confinement".into(),
            ));
        }
        opened.assert_stable()?;
        Ok(opened)
    }

    pub(crate) fn open_or_create_within(path: &Path, workspace: &Path) -> Result<Self> {
        let canonical_workspace = dunce::canonicalize(workspace)?;
        let opened = Self::open_or_create(path)?;
        let canonical_path = dunce::canonicalize(path)?;
        if !canonical_path.starts_with(&canonical_workspace) {
            return Err(ScienceError::Invalid(
                "project store root escapes the canonical workspace".into(),
            ));
        }
        let reopened = Self::open_verified_directory(canonical_path)?;
        if !windows_same_open_file(&opened.file, &reopened.file) {
            return Err(ScienceError::Invalid(
                "project store root identity changed during confinement".into(),
            ));
        }
        opened.assert_stable()?;
        Ok(opened)
    }

    /// Cross-process writer lock via `LockFileEx`.
    ///
    /// Opens a private regular file at the fixed lock name inside the retained
    /// store root, takes an exclusive byte-range lock via `LockFileEx`, and
    /// re-verifies the file identity before returning. The lock is released
    /// when the returned `ProjectWriteFileLock` is dropped.
    pub(crate) fn lock_project_writes(&self) -> Result<ProjectWriteFileLock> {
        self.assert_stable()?;
        let lock_path = self.path.join(PROJECT_WRITE_LOCK_FILE);
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&lock_path)?;
        windows_assert_regular_handle(&lock_path, &file)?;
        windows_assert_no_reparse_components(&lock_path)?;

        use windows_sys::Win32::Storage::FileSystem::{
            LockFileEx, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
        };
        let mut overlapped: windows_sys::Win32::System::IO::OVERLAPPED = unsafe {
            std::mem::zeroed()
        };
        let result = unsafe {
            LockFileEx(
                file.as_raw_fd() as _,
                LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                0,
                u32::MAX,
                0,
                &mut overlapped,
            )
        };
        if result == 0 {
            let error = std::io::Error::last_os_error();
            return Err(ScienceError::Io(error));
        }

        // Reverify the lock file identity post-lock
        let reopened = fs::File::open(&lock_path)?;
        windows_assert_regular_handle(&lock_path, &reopened)?;
        if !windows_same_open_file(&file, &reopened) {
            return Err(ScienceError::Invalid(
                "project store write lock identity changed during acquisition".into(),
            ));
        }
        Ok(ProjectWriteFileLock { _file: file })
    }

    pub(crate) fn read_optional(&self, relative: &Path) -> Result<Option<Vec<u8>>> {
        validate_relative(relative)?;
        let parent = match self.open_parent(relative, false) {
            Ok(parent) => parent,
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        parent.assert_stable()?;
        let path = parent.path.join(relative.file_name().unwrap());
        let mut file = match windows_open_regular(&path, false) {
            Ok(file) => file,
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        windows_assert_regular_handle(&path, &file)?;
        parent.assert_stable()?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(Some(bytes))
    }

    pub(crate) fn replace_atomic(&self, relative: &Path, bytes: &[u8]) -> Result<()> {
        self.publish(relative, bytes, false)
    }

    pub(crate) fn write_new_atomic(&self, relative: &Path, bytes: &[u8]) -> Result<()> {
        self.publish(relative, bytes, true)
    }

    fn publish(&self, relative: &Path, bytes: &[u8], create_only: bool) -> Result<()> {
        validate_relative(relative)?;
        let parent = self.open_parent(relative, true)?;
        parent.assert_stable()?;
        let target = parent.path.join(relative.file_name().unwrap());
        if target.exists() {
            let current = windows_open_regular(&target, false)?;
            windows_assert_regular_handle(&target, &current)?;
            if create_only {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "project record already exists",
                )
                .into());
            }
        }
        let temp = parent.path.join(format!(".project-{}.tmp", Uuid::new_v4()));
        let mut staged = windows_open_regular(&temp, true)?;
        let result = (|| -> Result<()> {
            windows_assert_regular_handle(&temp, &staged)?;
            staged.write_all(bytes)?;
            staged.sync_all()?;
            parent.assert_stable()?;
            if create_only {
                fs::hard_link(&temp, &target)?;
                fs::remove_file(&temp)?;
            } else {
                windows_replace_file(&temp, &target)?;
            }
            parent.assert_stable()?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&temp);
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn list_names(&self, relative: &Path) -> Result<Vec<OsString>> {
        let directory = match self.open_directory(relative) {
            Ok(directory) => directory,
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Vec::new());
            }
            Err(error) => return Err(error),
        };
        directory.assert_stable()?;
        let mut names = Vec::new();
        for entry in fs::read_dir(&directory.path)? {
            names.push(entry?.file_name());
        }
        directory.assert_stable()?;
        names.sort();
        Ok(names)
    }

    pub(crate) fn remove_file(&self, relative: &Path) -> Result<bool> {
        validate_relative(relative)?;
        let parent = match self.open_parent(relative, false) {
            Ok(parent) => parent,
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        parent.assert_stable()?;
        let path = parent.path.join(relative.file_name().unwrap());
        match fs::symlink_metadata(&path) {
            Ok(metadata) if windows_has_reparse_point(&metadata) => {
                return Err(ScienceError::Invalid(
                    "project record deletion refuses a reparse point".into(),
                ));
            }
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => {
                return Err(ScienceError::Invalid(
                    "project record deletion requires a regular file".into(),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        }
        let file = windows_open_regular(&path, false)?;
        windows_assert_regular_handle(&path, &file)?;
        parent.assert_stable()?;
        fs::remove_file(path)?;
        parent.assert_stable()?;
        Ok(true)
    }

    pub(crate) fn create_directory(&self, relative: &Path) -> Result<Self> {
        self.create_directories(relative)
    }

    pub(crate) fn snapshot_nofollow_bounded(
        &self,
        max_bytes: u64,
    ) -> Result<Vec<(PathBuf, Vec<u8>)>> {
        let mut files = Vec::new();
        let mut bytes_read = 0u64;
        self.snapshot_nofollow_into(Path::new(""), &mut files, &mut bytes_read, max_bytes)?;
        Ok(files)
    }

    fn snapshot_nofollow_into(
        &self,
        prefix: &Path,
        files: &mut Vec<(PathBuf, Vec<u8>)>,
        bytes_read: &mut u64,
        max_bytes: u64,
    ) -> Result<()> {
        self.assert_stable()?;
        let mut names = Vec::new();
        for entry in fs::read_dir(&self.path)? {
            names.push(entry?.file_name());
        }
        names.sort();
        for name in names {
            self.assert_stable()?;
            let relative = prefix.join(&name);
            let path = self.path.join(&name);
            let metadata = fs::symlink_metadata(&path)?;
            if windows_has_reparse_point(&metadata) {
                return Err(ScienceError::Invalid(format!(
                    "workflow output snapshot refuses reparse entry '{}'",
                    relative.display()
                )));
            }
            if metadata.is_dir() {
                let child = Self::open_verified_directory(path)?;
                child.snapshot_nofollow_into(&relative, files, bytes_read, max_bytes)?;
            } else if metadata.is_file() {
                let mut file = windows_open_regular(&path, false)?;
                windows_assert_regular_handle(&path, &file)?;
                self.assert_stable()?;
                let remaining = max_bytes.saturating_sub(*bytes_read);
                let mut bytes = Vec::new();
                file.take(remaining.saturating_add(1))
                    .read_to_end(&mut bytes)?;
                *bytes_read = bytes_read.checked_add(bytes.len() as u64).ok_or_else(|| {
                    ScienceError::Invalid("workflow output snapshot byte count overflowed".into())
                })?;
                if *bytes_read > max_bytes {
                    return Err(ScienceError::Invalid(format!(
                        "workflow output exceeds the admitted {max_bytes} byte cap"
                    )));
                }
                files.push((relative, bytes));
            } else {
                return Err(ScienceError::Invalid(format!(
                    "workflow output snapshot refuses special entry '{}'",
                    relative.display()
                )));
            }
        }
        self.assert_stable()?;
        Ok(())
    }

    fn open_parent(&self, relative: &Path, create: bool) -> Result<Self> {
        match relative.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => {
                if create {
                    self.create_directories(parent)
                } else {
                    self.open_directory(parent)
                }
            }
            _ => self.try_clone(),
        }
    }

    fn open_directory(&self, relative: &Path) -> Result<Self> {
        if relative.as_os_str().is_empty() {
            return self.try_clone();
        }
        validate_relative(relative)?;
        let mut current = self.try_clone()?;
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err(ScienceError::Invalid(
                    "project directory contains a non-normal component".into(),
                ));
            };
            current.assert_stable()?;
            current = Self::open_verified_directory(current.path.join(name))?;
        }
        Ok(current)
    }

    fn create_directories(&self, relative: &Path) -> Result<Self> {
        if relative.as_os_str().is_empty() {
            return self.try_clone();
        }
        validate_relative(relative)?;
        let mut current = self.try_clone()?;
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err(ScienceError::Invalid(
                    "project directory contains a non-normal component".into(),
                ));
            };
            current.assert_stable()?;
            let child = current.path.join(name);
            match fs::create_dir(&child) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
            current = Self::open_verified_directory(child)?;
        }
        Ok(current)
    }

    fn open_verified_directory(path: PathBuf) -> Result<Self> {
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_dir() || windows_has_reparse_point(&metadata) {
            return Err(ScienceError::Invalid(
                "project directory must not be a Windows reparse point".into(),
            ));
        }
        let file = windows_open_directory(&path)?;
        let directory = Self { path, file };
        directory.assert_stable()?;
        Ok(directory)
    }

    fn assert_stable(&self) -> Result<()> {
        let reopened = windows_open_directory(&self.path)?;
        if !windows_same_open_file(&self.file, &reopened)
            || !windows_final_handle_path_matches(&self.path, &self.file)
        {
            return Err(ScienceError::Invalid(
                "project directory identity changed during operation".into(),
            ));
        }
        Ok(())
    }

    fn try_clone(&self) -> Result<Self> {
        Ok(Self {
            path: self.path.clone(),
            file: self.file.try_clone()?,
        })
    }

    pub(crate) fn identity(&self) -> Result<crate::StoreRootIdentity> {
        let identity = windows_file_identity(&self.file).ok_or_else(|| {
            ScienceError::Invalid("cannot resolve project store root identity".into())
        })?;
        Ok(crate::StoreRootIdentity::Windows {
            volume: identity.volume,
            index: identity.index,
        })
    }
}

#[cfg(windows)]
fn windows_open_directory(path: &Path) -> Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt as _;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    let mut options = fs::OpenOptions::new();
    options
        .read(true)
        // Deliberately omit FILE_SHARE_DELETE: a retained directory used as a
        // capability must not be renamed or deleted behind the handle.
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    Ok(options.open(path)?)
}

#[cfg(windows)]
fn windows_assert_no_reparse_components(path: &Path) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if !matches!(component, Component::Normal(_)) {
            continue;
        }
        let metadata = fs::symlink_metadata(&current)?;
        if windows_has_reparse_point(&metadata) {
            return Err(ScienceError::Invalid(format!(
                "project store path contains Windows reparse component '{}'",
                current.display()
            )));
        }
        if !metadata.is_dir() {
            return Err(ScienceError::Invalid(format!(
                "project store path component is not a directory: '{}'",
                current.display()
            )));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn windows_open_regular(path: &Path, create: bool) -> Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt as _;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    let mut options = fs::OpenOptions::new();
    if create {
        options.write(true).create_new(true);
    } else {
        options.read(true);
    }
    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    Ok(options.open(path)?)
}

#[cfg(windows)]
fn windows_replace_file(source: &Path, target: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt as _;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "MoveFileExW"]
        fn move_file_ex(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let source: Vec<u16> = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let target: Vec<u16> = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    if source[..source.len() - 1].contains(&0) || target[..target.len() - 1].contains(&0) {
        return Err(ScienceError::Invalid(
            "project record path contains NUL".into(),
        ));
    }
    // SAFETY: both UTF-16 paths are NUL-terminated and live for the call.
    if unsafe {
        move_file_ex(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(std::io::Error::last_os_error().into())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn windows_has_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(windows)]
fn windows_assert_regular_handle(path: &Path, file: &fs::File) -> Result<()> {
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || windows_has_reparse_point(&metadata)
        || !windows_final_handle_path_matches(path, file)
    {
        return Err(ScienceError::Invalid(
            "project record must be a stable non-reparse regular file".into(),
        ));
    }
    Ok(())
}

#[cfg(windows)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct WindowsIdentity {
    volume: u32,
    index: u64,
}

#[cfg(windows)]
fn windows_same_open_file(left: &fs::File, right: &fs::File) -> bool {
    windows_file_identity(left)
        .zip(windows_file_identity(right))
        .is_some_and(|(left, right)| left == right)
}

#[cfg(windows)]
fn windows_file_identity(file: &fs::File) -> Option<WindowsIdentity> {
    use std::os::windows::io::AsRawHandle as _;
    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[repr(C)]
    struct Information {
        attributes: u32,
        creation: FileTime,
        access: FileTime,
        write: FileTime,
        volume: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "GetFileInformationByHandle"]
        fn get_information(handle: *mut std::ffi::c_void, info: *mut Information) -> i32;
    }
    let mut info = std::mem::MaybeUninit::uninit();
    // SAFETY: the borrowed handle is live and info has the documented layout.
    if unsafe { get_information(file.as_raw_handle().cast(), info.as_mut_ptr()) } == 0 {
        return None;
    }
    // SAFETY: success initialized the output.
    let info = unsafe { info.assume_init() };
    Some(WindowsIdentity {
        volume: info.volume,
        index: (u64::from(info.index_high) << 32) | u64::from(info.index_low),
    })
}

#[cfg(windows)]
fn windows_final_handle_path_matches(path: &Path, file: &fs::File) -> bool {
    use std::os::windows::{ffi::OsStringExt as _, io::AsRawHandle as _};
    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "GetFinalPathNameByHandleW"]
        fn get_final_path(
            handle: *mut std::ffi::c_void,
            path: *mut u16,
            path_len: u32,
            flags: u32,
        ) -> u32;
    }
    let handle = file.as_raw_handle().cast();
    // SAFETY: a null output is the documented size query.
    let needed = unsafe { get_final_path(handle, std::ptr::null_mut(), 0, 0) };
    if needed == 0 {
        return false;
    }
    let mut buffer = vec![0_u16; needed as usize + 1];
    // SAFETY: the buffer has the advertised writable capacity.
    let written = unsafe { get_final_path(handle, buffer.as_mut_ptr(), buffer.len() as u32, 0) };
    if written == 0 || written as usize >= buffer.len() {
        return false;
    }
    let handle_path = PathBuf::from(OsString::from_wide(&buffer[..written as usize]));
    dunce::canonicalize(path).is_ok_and(|canonical| canonical == dunce::simplified(&handle_path))
}

#[cfg(not(any(unix, windows)))]
#[derive(Debug)]
pub(crate) struct PinnedDirectory;

#[cfg(not(any(unix, windows)))]
impl PinnedDirectory {
    pub(crate) fn identity(&self) -> Result<crate::StoreRootIdentity> {
        Err(ScienceError::FeatureDisabled(
            "confined project-store identity has no backend for this platform".into(),
        ))
    }

    pub(crate) fn open_or_create(_path: &Path) -> Result<Self> {
        Err(ScienceError::FeatureDisabled(
            "confined project-store I/O has no backend for this platform".into(),
        ))
    }

    pub(crate) fn open_or_create_within(_path: &Path, _workspace: &Path) -> Result<Self> {
        Err(ScienceError::FeatureDisabled(
            "confined project-store I/O has no backend for this platform".into(),
        ))
    }

    pub(crate) fn open_existing_within(_path: &Path, _workspace: &Path) -> Result<Self> {
        Err(ScienceError::FeatureDisabled(
            "confined project-store I/O has no backend for this platform".into(),
        ))
    }

    /// Unsupported platforms retain only the in-process mutex.
    pub(crate) fn lock_project_writes(&self) -> Result<ProjectWriteFileLock> {
        Ok(ProjectWriteFileLock { _process_only: () })
    }

    pub(crate) fn read_optional(&self, _relative: &Path) -> Result<Option<Vec<u8>>> {
        Err(ScienceError::FeatureDisabled(
            "confined project-store I/O has no backend for this platform".into(),
        ))
    }

    pub(crate) fn replace_atomic(&self, _relative: &Path, _bytes: &[u8]) -> Result<()> {
        Err(ScienceError::FeatureDisabled(
            "confined project-store I/O has no backend for this platform".into(),
        ))
    }

    pub(crate) fn write_new_atomic(&self, _relative: &Path, _bytes: &[u8]) -> Result<()> {
        Err(ScienceError::FeatureDisabled(
            "confined project-store I/O has no backend for this platform".into(),
        ))
    }

    pub(crate) fn list_names(&self, _relative: &Path) -> Result<Vec<OsString>> {
        Err(ScienceError::FeatureDisabled(
            "confined project-store I/O has no backend for this platform".into(),
        ))
    }

    pub(crate) fn remove_file(&self, _relative: &Path) -> Result<bool> {
        Err(ScienceError::FeatureDisabled(
            "confined project-store I/O has no backend for this platform".into(),
        ))
    }

    pub(crate) fn create_directory(&self, _relative: &Path) -> Result<Self> {
        Err(ScienceError::FeatureDisabled(
            "confined project-store I/O has no backend for this platform".into(),
        ))
    }

    pub(crate) fn snapshot_nofollow_bounded(
        &self,
        _max_bytes: u64,
    ) -> Result<Vec<(PathBuf, Vec<u8>)>> {
        Err(ScienceError::FeatureDisabled(
            "confined project-store I/O has no backend for this platform".into(),
        ))
    }
}
