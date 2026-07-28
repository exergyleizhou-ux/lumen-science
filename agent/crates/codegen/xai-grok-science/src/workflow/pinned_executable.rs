//! Immutable executable identity for admitted workflow kernels.
//!
//! A path is only a lookup hint: another process can replace the object at
//! that path after admission. `PinnedExecutable` therefore owns a private
//! snapshot of the bytes that were admitted. Ordinary Linux probes execute
//! that snapshot. A Landlock-confined workflow cannot because path-beneath
//! rules reject anonymous memfds; that narrower path re-hashes an exact
//! root-owned protected executable, binds its inode into Landlock, and
//! executes it only while it matches the sealed admission snapshot.
//! `PinnedExecutable` deliberately has no `Serialize` implementation; the
//! live capability cannot be reconstructed from a path or digest.
//!
//! Platform backends:
//! - macOS has no `fexecve`, and inherited `/dev/fd` nodes are not executable.
//!   It accepts only a root-owned executable whose file and every ancestor are
//!   non-writable by group/other. The retained handle, inode and digest are
//!   revalidated immediately before spawn. User-writable Homebrew/venv paths
//!   fail closed until a signed managed runtime exists.
//! - Linux copies into a `memfd`, re-hashes it, and applies all write/grow/shrink
//!   seals before the capability is returned. Landlock workflow execution
//!   additionally requires a matching root-owned protected system path.
//! - Other platforms fail closed. In particular, there is no fallback that
//!   executes an unverified caller path.

use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fmt, fs,
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Output},
    sync::Arc,
};

#[cfg(target_os = "linux")]
use std::io::Write;
#[cfg(target_os = "linux")]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(target_os = "linux")]
use std::os::unix::fs::FileTypeExt;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;

/// The OS primitive retaining the admitted executable bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinnedExecutableBackend {
    /// A root-owned path whose complete ancestor chain is non-writable.
    MacOsProtectedPath,
    /// A write-sealed anonymous admission snapshot.
    ///
    /// Ordinary probes execute it through `/proc/self/fd/N`. Landlock
    /// workflow execution uses a separately revalidated root-owned inode with
    /// the same digest because anonymous inodes cannot be path-beneath rules.
    LinuxSealedMemfd,
}

impl fmt::Display for PinnedExecutableBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MacOsProtectedPath => f.write_str("macos-protected-path"),
            Self::LinuxSealedMemfd => f.write_str("linux-sealed-admission+protected-path-exec"),
        }
    }
}

/// A failure to obtain an immutable executable capability.
#[derive(Debug)]
pub enum PinExecutableError {
    PathNotAbsolute {
        path: PathBuf,
    },
    Canonicalize {
        path: PathBuf,
        source: io::Error,
    },
    Open {
        path: PathBuf,
        source: io::Error,
    },
    NotRegularFile {
        path: PathBuf,
    },
    NotExecutable {
        path: PathBuf,
    },
    MacOsPathNotProtected {
        path: PathBuf,
    },
    PathChangedDuringOpen {
        path: PathBuf,
    },
    SnapshotIo {
        operation: &'static str,
        source: io::Error,
    },
    SourceChangedDuringPin {
        copied_sha256: String,
        observed_sha256: String,
    },
    SnapshotDigestMismatch {
        copied_sha256: String,
        snapshot_sha256: String,
    },
    SnapshotIdentityMismatch,
    SnapshotSealingFailed {
        detail: String,
    },
    CommandSetup {
        source: io::Error,
    },
    UnsupportedPlatform {
        platform: &'static str,
    },
}

impl fmt::Display for PinExecutableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PathNotAbsolute { path } => {
                write!(f, "executable path '{}' is not absolute", path.display())
            }
            Self::Canonicalize { path, source } => write!(
                f,
                "could not canonicalize executable '{}': {source}",
                path.display()
            ),
            Self::Open { path, source } => {
                write!(
                    f,
                    "could not open executable '{}': {source}",
                    path.display()
                )
            }
            Self::NotRegularFile { path } => {
                write!(f, "executable '{}' is not a regular file", path.display())
            }
            Self::NotExecutable { path } => {
                write!(f, "executable '{}' has no execute bit", path.display())
            }
            Self::MacOsPathNotProtected { path } => write!(
                f,
                "macOS workflow executable '{}' is not a root-owned, non-writable protected path",
                path.display()
            ),
            Self::PathChangedDuringOpen { path } => write!(
                f,
                "executable '{}' changed between inspection and open",
                path.display()
            ),
            Self::SnapshotIo { operation, source } => {
                write!(f, "could not {operation} executable snapshot: {source}")
            }
            Self::SourceChangedDuringPin {
                copied_sha256,
                observed_sha256,
            } => write!(
                f,
                "source executable changed while it was pinned: copied {copied_sha256}, observed {observed_sha256}"
            ),
            Self::SnapshotDigestMismatch {
                copied_sha256,
                snapshot_sha256,
            } => write!(
                f,
                "executable snapshot digest mismatch: copied {copied_sha256}, snapshot {snapshot_sha256}"
            ),
            Self::SnapshotIdentityMismatch => {
                f.write_str("temporary executable was replaced before it could be unlinked")
            }
            Self::SnapshotSealingFailed { detail } => {
                write!(f, "could not seal executable snapshot: {detail}")
            }
            Self::CommandSetup { source } => {
                write!(f, "could not prepare pinned executable command: {source}")
            }
            Self::UnsupportedPlatform { platform } => write!(
                f,
                "pinned executable snapshots are unsupported on {platform}; refusing path execution"
            ),
        }
    }
}

impl Error for PinExecutableError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Canonicalize { source, .. }
            | Self::Open { source, .. }
            | Self::SnapshotIo { source, .. }
            | Self::CommandSetup { source } => Some(source),
            _ => None,
        }
    }
}

/// A live, non-serializable capability for one immutable executable image.
///
/// `canonical_path` is retained for display and evidence only. Process launch
/// uses `snapshot`, never that path.
#[derive(Debug)]
pub struct PinnedExecutable {
    requested_path: PathBuf,
    canonical_path: PathBuf,
    sha256: String,
    snapshot: Arc<PinnedSnapshot>,
}

#[derive(Debug)]
struct PinnedSnapshot {
    file: File,
    backend: PinnedExecutableBackend,
    #[cfg(target_os = "macos")]
    execution_path: PathBuf,
}

impl PinnedExecutable {
    /// Resolve, validate, copy, verify, and retain an executable.
    pub fn pin(path: impl AsRef<Path>) -> Result<Self, PinExecutableError> {
        let requested = path.as_ref();
        if !requested.is_absolute() {
            return Err(PinExecutableError::PathNotAbsolute {
                path: requested.to_path_buf(),
            });
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = requested;
            return Err(PinExecutableError::UnsupportedPlatform {
                platform: std::env::consts::OS,
            });
        }

        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            let requested_path = requested.to_path_buf();
            let canonical_path = dunce::canonicalize(requested).map_err(|source| {
                PinExecutableError::Canonicalize {
                    path: requested.to_path_buf(),
                    source,
                }
            })?;
            let inspected = fs::symlink_metadata(&canonical_path).map_err(|source| {
                PinExecutableError::Open {
                    path: canonical_path.clone(),
                    source,
                }
            })?;
            if !inspected.file_type().is_file() {
                return Err(PinExecutableError::NotRegularFile {
                    path: canonical_path,
                });
            }
            if inspected.permissions().mode() & 0o111 == 0 {
                return Err(PinExecutableError::NotExecutable {
                    path: canonical_path,
                });
            }

            let mut source = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
                .open(&canonical_path)
                .map_err(|source| PinExecutableError::Open {
                    path: canonical_path.clone(),
                    source,
                })?;
            let opened = source
                .metadata()
                .map_err(|source| PinExecutableError::Open {
                    path: canonical_path.clone(),
                    source,
                })?;
            if !opened.is_file() {
                return Err(PinExecutableError::NotRegularFile {
                    path: canonical_path,
                });
            }
            if opened.permissions().mode() & 0o111 == 0 {
                return Err(PinExecutableError::NotExecutable {
                    path: canonical_path,
                });
            }
            if inspected.dev() != opened.dev() || inspected.ino() != opened.ino() {
                return Err(PinExecutableError::PathChangedDuringOpen {
                    path: canonical_path,
                });
            }

            #[cfg(target_os = "macos")]
            let (snapshot, copied_sha256) = {
                verify_macos_protected_path(&canonical_path)?;
                let copied_sha256 =
                    hash_file(&mut source).map_err(|source| PinExecutableError::SnapshotIo {
                        operation: "hash protected executable",
                        source,
                    })?;
                let retained =
                    source
                        .try_clone()
                        .map_err(|source| PinExecutableError::SnapshotIo {
                            operation: "retain protected executable",
                            source,
                        })?;
                (
                    PinnedSnapshot {
                        file: retained,
                        backend: PinnedExecutableBackend::MacOsProtectedPath,
                        execution_path: canonical_path.clone(),
                    },
                    copied_sha256,
                )
            };
            #[cfg(target_os = "linux")]
            let (snapshot, copied_sha256) = {
                let executable_mode = opened.permissions().mode() & 0o555;
                create_snapshot(&mut source, executable_mode)?
            };

            // Re-read the still-open source. A rename is harmless, but byte
            // mutation during the copy must not bind admission to a torn read.
            let observed_sha256 =
                hash_file(&mut source).map_err(|source| PinExecutableError::SnapshotIo {
                    operation: "re-hash source",
                    source,
                })?;
            if copied_sha256 != observed_sha256 {
                return Err(PinExecutableError::SourceChangedDuringPin {
                    copied_sha256,
                    observed_sha256,
                });
            }

            Ok(Self {
                requested_path,
                canonical_path,
                sha256: observed_sha256,
                snapshot: Arc::new(snapshot),
            })
        }
    }

    /// Canonical source spelling retained for evidence and diagnostics.
    ///
    /// A Linux Landlock workflow revalidates this protected path against the
    /// sealed admission snapshot and binds the exact inode before spawn.
    /// Ordinary Linux probes execute the sealed snapshot directly.
    pub fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    /// Whether `path` is one of the immutable source spellings captured while
    /// this capability was pinned.
    ///
    /// This is deliberately lexical. Re-resolving the caller's path after an
    /// approval wait would reopen the very TOCTOU window the capability closes.
    pub fn matches_source_path(&self, path: &Path) -> bool {
        path == self.requested_path || path == self.canonical_path
    }

    /// SHA-256 of the immutable admitted bytes.
    ///
    /// A sandboxed Linux workflow may execute only a protected inode that is
    /// re-hashed to this exact value.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn backend(&self) -> PinnedExecutableBackend {
        self.snapshot.backend
    }

    /// Build a command whose program is the retained snapshot.
    ///
    /// Crate-internal callers may configure argv, environment, stdio, and
    /// working directory through [`PinnedCommand::command_mut`]. The wrapper
    /// keeps the underlying capability alive even if this `PinnedExecutable`
    /// is dropped, and process creation remains private to the wrapper.
    pub fn spawn_command(&self) -> Result<PinnedCommand, PinExecutableError> {
        #[cfg(target_os = "macos")]
        {
            let execution_path = self.snapshot.execution_path.as_path();
            verify_macos_protected_executable(&self.snapshot, execution_path, &self.sha256)?;
            Ok(PinnedCommand {
                command: Command::new(execution_path),
                inherited_snapshot: None,
                snapshot: Arc::clone(&self.snapshot),
                execution_path: execution_path.to_path_buf(),
                expected_sha256: self.sha256.clone(),
            })
        }

        #[cfg(target_os = "linux")]
        {
            let inherited_snapshot = duplicate_inheritable(&self.snapshot.file)?;
            let inherited_fd = inherited_snapshot.as_raw_fd();
            let mut command = Command::new(format!("/proc/self/fd/{inherited_fd}"));
            unsafe {
                // SAFETY: the closure performs no work. Registering a
                // `pre_exec` hook forces the fork/exec path on Unix so the
                // explicitly inheritable descriptor is not filtered by a
                // platform `posix_spawn` close-fds policy.
                command.pre_exec(|| Ok(()));
            }
            Ok(PinnedCommand {
                command,
                inherited_snapshot: Some(inherited_snapshot),
                snapshot: Arc::clone(&self.snapshot),
                execution_path: self.canonical_path.clone(),
                expected_sha256: self.sha256.clone(),
                verify_protected_execution_path: false,
            })
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            Err(PinExecutableError::UnsupportedPlatform {
                platform: std::env::consts::OS,
            })
        }
    }

    /// Build the only Linux command admitted for a Landlock-confined workflow.
    ///
    /// This is separate from [`Self::spawn_command`] so the protected program
    /// is chosen before a caller configures argv, environment or stdio. It
    /// never creates an inheritable memfd duplicate; the sealed snapshot stays
    /// in the parent as the admission reference.
    #[cfg(target_os = "linux")]
    pub(crate) fn spawn_linux_sandboxed_command(
        &self,
        output_fd: std::os::fd::RawFd,
    ) -> io::Result<PinnedCommand> {
        use std::os::fd::AsRawFd as _;
        use std::os::unix::process::CommandExt as _;

        // Build every allocating part in the parent. The pre-exec child may
        // use only async-signal-safe syscalls.
        let seccomp_filter = build_linux_seccomp_filter()?;
        let read_capabilities = open_linux_runtime_read_capabilities()?;
        let output_capability = open_linux_directory_path_capability(output_fd)?;
        let executable_capability =
            open_verified_linux_executable_capability(&self.canonical_path, &self.sha256)
                .map_err(io::Error::other)?;
        let mut command = Command::new(&self.canonical_path);
        // SAFETY: the closure captures parent-built BPF instructions and
        // parent-opened descriptor capabilities, then performs only syscalls.
        unsafe {
            command.pre_exec(move || {
                apply_linux_landlock_and_seccomp(
                    output_capability.as_raw_fd(),
                    executable_capability.as_raw_fd(),
                    &read_capabilities,
                    &seccomp_filter,
                )
            });
        }
        Ok(PinnedCommand {
            command,
            inherited_snapshot: None,
            snapshot: Arc::clone(&self.snapshot),
            execution_path: self.canonical_path.clone(),
            expected_sha256: self.sha256.clone(),
            verify_protected_execution_path: true,
        })
    }
}

#[cfg(target_os = "macos")]
fn verify_macos_protected_path(path: &Path) -> Result<(), PinExecutableError> {
    let mut current = Some(path);
    while let Some(component) = current {
        let metadata =
            fs::symlink_metadata(component).map_err(|source| PinExecutableError::Open {
                path: component.to_path_buf(),
                source,
            })?;
        if metadata.file_type().is_symlink()
            || metadata.uid() != 0
            || metadata.permissions().mode() & 0o022 != 0
        {
            return Err(PinExecutableError::MacOsPathNotProtected {
                path: component.to_path_buf(),
            });
        }
        current = component.parent().filter(|parent| *parent != component);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn verify_macos_protected_executable(
    snapshot: &PinnedSnapshot,
    path: &Path,
    expected_sha256: &str,
) -> Result<(), PinExecutableError> {
    let held = snapshot
        .file
        .metadata()
        .map_err(|source| PinExecutableError::CommandSetup { source })?;
    let inspected =
        fs::symlink_metadata(path).map_err(|source| PinExecutableError::CommandSetup { source })?;
    if !inspected.file_type().is_file()
        || inspected.dev() != held.dev()
        || inspected.ino() != held.ino()
    {
        return Err(PinExecutableError::SnapshotIdentityMismatch);
    }

    let mut reopened = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)
        .map_err(|source| PinExecutableError::CommandSetup { source })?;
    let reopened_meta = reopened
        .metadata()
        .map_err(|source| PinExecutableError::CommandSetup { source })?;
    if reopened_meta.dev() != held.dev() || reopened_meta.ino() != held.ino() {
        return Err(PinExecutableError::SnapshotIdentityMismatch);
    }
    let observed =
        hash_file(&mut reopened).map_err(|source| PinExecutableError::CommandSetup { source })?;
    if observed != expected_sha256 {
        return Err(PinExecutableError::SnapshotDigestMismatch {
            copied_sha256: expected_sha256.to_owned(),
            snapshot_sha256: observed,
        });
    }
    Ok(())
}

/// Configurable process builder that keeps the executable descriptor alive.
#[derive(Debug)]
pub struct PinnedCommand {
    command: Command,
    // Linux sets this to a descriptor with CLOEXEC cleared. macOS executes the
    // protected retained path because its `/dev/fd` nodes are not executable.
    inherited_snapshot: Option<File>,
    snapshot: Arc<PinnedSnapshot>,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    execution_path: PathBuf,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    expected_sha256: String,
    #[cfg(target_os = "linux")]
    verify_protected_execution_path: bool,
}

impl PinnedCommand {
    pub(crate) fn command_mut(&mut self) -> &mut Command {
        &mut self.command
    }

    /// Apply hard kernel resource ceilings in the child before any executable
    /// image starts. Every requested limit is clamped to the current hard limit
    /// so an already stricter parent policy is preserved.
    #[cfg(unix)]
    pub(crate) fn apply_resource_limits(
        &mut self,
        address_space_bytes: u64,
        cpu_seconds: u64,
        file_bytes: u64,
        open_files: u32,
    ) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        let address_space = libc::rlim_t::try_from(address_space_bytes).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "address-space limit does not fit rlim_t",
            )
        })?;
        #[cfg(not(target_os = "linux"))]
        let _ = address_space_bytes;
        let cpu = libc::rlim_t::try_from(cpu_seconds).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "CPU limit does not fit rlim_t")
        })?;
        let file = libc::rlim_t::try_from(file_bytes).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "file-size limit does not fit rlim_t",
            )
        })?;
        let nofile = libc::rlim_t::from(open_files);

        use std::os::unix::process::CommandExt as _;
        // SAFETY: the closure performs only async-signal-safe setrlimit calls
        // and captures plain integers.
        unsafe {
            self.command.pre_exec(move || {
                #[cfg(target_os = "linux")]
                set_bounded_rlimit(libc::RLIMIT_AS, address_space)?;
                set_bounded_rlimit(libc::RLIMIT_CPU, cpu)?;
                set_bounded_rlimit(libc::RLIMIT_FSIZE, file)?;
                set_bounded_rlimit(libc::RLIMIT_NOFILE, nofile)?;
                set_bounded_rlimit(libc::RLIMIT_CORE, 0)?;
                Ok(())
            });
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn enable_os_sandbox(
        &mut self,
        writable_root: &Path,
        max_memory_mb: u64,
    ) -> io::Result<()> {
        const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";
        const TASKPOLICY: &str = "/usr/sbin/taskpolicy";
        const SYSTEM_LIBRARY: &str = "/System/Library";
        const SYSTEM_DYLIBS: &str = "/usr/lib";

        if max_memory_mb == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "macOS taskpolicy memory limit must be greater than zero",
            ));
        }
        verify_macos_protected_path(Path::new(SANDBOX_EXEC)).map_err(io::Error::other)?;
        verify_macos_protected_path(Path::new(TASKPOLICY)).map_err(io::Error::other)?;

        let writable_root = writable_root.to_str().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "macOS sandbox output path must be valid UTF-8",
            )
        })?;
        if writable_root.contains(['\0', '\n', '\r']) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "macOS sandbox output path contains a control character",
            ));
        }
        let runtime_root = self
            .execution_path
            .ancestors()
            .find(|ancestor| {
                ancestor
                    .extension()
                    .is_some_and(|extension| extension == "framework")
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "macOS Python executable is not inside a protected framework runtime",
                )
            })?;
        verify_macos_protected_path(runtime_root).map_err(io::Error::other)?;

        let read_roots = [
            Path::new(SYSTEM_LIBRARY),
            Path::new(SYSTEM_DYLIBS),
            runtime_root,
        ];
        let mut parents = std::collections::BTreeSet::new();
        parents.insert(PathBuf::from("/"));
        for path in read_roots
            .iter()
            .copied()
            .chain([self.execution_path.as_path(), writable_root.as_ref()])
        {
            parents.extend(path.ancestors().map(Path::to_path_buf));
        }

        let mut profile = String::from(
            "(version 1)\n\
             (deny default)\n\
             (allow process-info* (target self))\n\
             (allow signal (target self))\n\
             (allow sysctl-read)\n\
             (allow system-info)\n\
             (allow mach-task-name)\n\
             (allow system-fsctl)\n\
             (deny network*)\n",
        );
        push_macos_rule(
            &mut profile,
            "allow process-exec",
            "literal",
            &self.execution_path,
        )?;
        for parent in parents {
            push_macos_rule(&mut profile, "allow file-read*", "literal", &parent)?;
        }
        for root in read_roots {
            push_macos_rule(&mut profile, "allow file-read*", "subpath", root)?;
            push_macos_rule(&mut profile, "allow file-map-executable", "subpath", root)?;
        }
        for device in ["/dev/null", "/dev/urandom", "/dev/random"] {
            push_macos_rule(
                &mut profile,
                "allow file-read*",
                "literal",
                Path::new(device),
            )?;
        }
        push_macos_rule(
            &mut profile,
            "allow file-write*",
            "literal",
            Path::new("/dev/null"),
        )?;
        push_macos_rule(
            &mut profile,
            "allow file-read*",
            "subpath",
            Path::new(writable_root),
        )?;
        push_macos_rule(
            &mut profile,
            "allow file-write*",
            "subpath",
            Path::new(writable_root),
        )?;

        let mut constrained = Command::new(TASKPOLICY);
        constrained
            .args(["-m", &max_memory_mb.to_string(), "-P", "kill", SANDBOX_EXEC])
            .arg("-p")
            .arg(profile)
            .arg(&self.execution_path);
        self.command = constrained;
        Ok(())
    }

    pub fn spawn(&mut self) -> io::Result<Child> {
        self.verify_immediately_before_spawn()?;
        let _keep_alive = &self.inherited_snapshot;
        let _keep_snapshot_alive = &self.snapshot;
        self.command.spawn()
    }

    pub fn output(&mut self) -> io::Result<Output> {
        self.verify_immediately_before_spawn()?;
        let _keep_alive = &self.inherited_snapshot;
        let _keep_snapshot_alive = &self.snapshot;
        self.command.output()
    }

    pub fn status(&mut self) -> io::Result<ExitStatus> {
        self.verify_immediately_before_spawn()?;
        let _keep_alive = &self.inherited_snapshot;
        let _keep_snapshot_alive = &self.snapshot;
        self.command.status()
    }

    fn verify_immediately_before_spawn(&self) -> io::Result<()> {
        #[cfg(target_os = "macos")]
        {
            verify_macos_protected_executable(
                &self.snapshot,
                &self.execution_path,
                &self.expected_sha256,
            )
            .map_err(io::Error::other)?;
        }
        #[cfg(target_os = "linux")]
        {
            if self.verify_protected_execution_path {
                open_verified_linux_executable_capability(
                    &self.execution_path,
                    &self.expected_sha256,
                )
                .map_err(io::Error::other)?;
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
type RlimitResource = libc::__rlimit_resource_t;
#[cfg(all(unix, not(target_os = "linux")))]
type RlimitResource = libc::c_int;

#[cfg(unix)]
fn set_bounded_rlimit(resource: RlimitResource, requested: libc::rlim_t) -> io::Result<()> {
    let mut current = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: both pointers refer to initialized storage owned by this frame.
    if unsafe { libc::getrlimit(resource, &mut current) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let bounded = requested.min(current.rlim_cur).min(current.rlim_max);
    let limit = libc::rlimit {
        rlim_cur: bounded,
        rlim_max: bounded,
    };
    // SAFETY: `limit` is initialized and the resource constant comes from libc.
    if unsafe { libc::setrlimit(resource, &limit) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn push_macos_rule(
    profile: &mut String,
    operation: &str,
    filter: &str,
    path: &Path,
) -> io::Result<()> {
    let path = path.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "macOS sandbox path must be valid UTF-8",
        )
    })?;
    if path.contains(['\0', '\n', '\r']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "macOS sandbox path contains a control character",
        ));
    }
    let escaped = path.replace('\\', "\\\\").replace('"', "\\\"");
    use std::fmt::Write as _;
    writeln!(profile, "({operation} ({filter} \"{escaped}\"))")
        .map_err(|_| io::Error::other("could not construct macOS sandbox profile"))
}

#[cfg(target_os = "linux")]
fn apply_linux_landlock_and_seccomp(
    output_fd: std::os::fd::RawFd,
    executable_fd: std::os::fd::RawFd,
    read_capabilities: &[LinuxPathCapability],
    seccomp_filter: &LinuxSeccompFilter,
) -> io::Result<()> {
    apply_linux_landlock(output_fd, executable_fd, read_capabilities)?;
    apply_linux_seccomp(seccomp_filter)
}

#[cfg(target_os = "linux")]
struct LinuxPathCapability {
    file: File,
    allowed_access: u64,
}

#[cfg(target_os = "linux")]
fn open_linux_directory_path_capability(directory_fd: std::os::fd::RawFd) -> io::Result<File> {
    let path_fd = unsafe {
        // SAFETY: `directory_fd` is a retained live directory capability and
        // the relative path is a static NUL-terminated string. The returned
        // descriptor is checked before ownership is transferred.
        libc::openat(
            directory_fd,
            c".".as_ptr(),
            libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if path_fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful openat returned a new descriptor owned by this value.
    Ok(unsafe { File::from_raw_fd(path_fd) })
}

#[cfg(target_os = "linux")]
fn open_linux_path_capability(path: &Path) -> io::Result<File> {
    let inspected = fs::symlink_metadata(path)?;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_PATH | libc::O_CLOEXEC | libc::O_NOFOLLOW);
    let capability = options.open(path)?;
    let opened = capability.metadata()?;
    if inspected.dev() != opened.dev() || inspected.ino() != opened.ino() {
        return Err(io::Error::other(
            "Linux runtime path changed while its capability was retained",
        ));
    }
    Ok(capability)
}

#[cfg(target_os = "linux")]
fn open_verified_linux_executable_capability(
    path: &Path,
    expected_sha256: &str,
) -> Result<File, PinExecutableError> {
    // The runtime read capability set is intentionally limited to the system
    // closure below (/usr, /lib, /lib64 and loader/device leaves). Refuse a
    // protected-looking interpreter elsewhere instead of letting it fail
    // later under a mismatched, undocumented runtime closure.
    if !path.starts_with("/usr") {
        return Err(PinExecutableError::CommandSetup {
            source: io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "Linux workflow executable is outside the admitted /usr runtime: {}",
                    path.display()
                ),
            ),
        });
    }
    let mut current = Some(path);
    while let Some(component) = current {
        let metadata =
            fs::symlink_metadata(component).map_err(|source| PinExecutableError::Open {
                path: component.to_path_buf(),
                source,
            })?;
        if metadata.file_type().is_symlink()
            || metadata.uid() != 0
            || metadata.permissions().mode() & 0o022 != 0
        {
            return Err(PinExecutableError::CommandSetup {
                source: io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!(
                        "Linux workflow executable is not root-owned and protected: {}",
                        component.display()
                    ),
                ),
            });
        }
        current = component.parent().filter(|parent| *parent != component);
    }

    let mut reopened = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)
        .map_err(|source| PinExecutableError::CommandSetup { source })?;
    let metadata = reopened
        .metadata()
        .map_err(|source| PinExecutableError::CommandSetup { source })?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err(PinExecutableError::NotExecutable {
            path: path.to_path_buf(),
        });
    }
    let mut magic = [0u8; 4];
    reopened
        .read_exact(&mut magic)
        .map_err(|source| PinExecutableError::CommandSetup { source })?;
    if magic != *b"\x7fELF" {
        return Err(PinExecutableError::CommandSetup {
            source: io::Error::new(
                io::ErrorKind::InvalidData,
                "Linux workflow executable must be a native ELF image; shebang chains are not admitted",
            ),
        });
    }
    let observed =
        hash_file(&mut reopened).map_err(|source| PinExecutableError::CommandSetup { source })?;
    if observed != expected_sha256 {
        return Err(PinExecutableError::SnapshotDigestMismatch {
            copied_sha256: expected_sha256.to_owned(),
            snapshot_sha256: observed,
        });
    }
    let capability = open_linux_path_capability(path)
        .map_err(|source| PinExecutableError::CommandSetup { source })?;
    let capability_metadata = capability
        .metadata()
        .map_err(|source| PinExecutableError::CommandSetup { source })?;
    if metadata.dev() != capability_metadata.dev() || metadata.ino() != capability_metadata.ino() {
        return Err(PinExecutableError::PathChangedDuringOpen {
            path: path.to_path_buf(),
        });
    }
    Ok(capability)
}

#[cfg(target_os = "linux")]
fn open_linux_runtime_read_capabilities() -> io::Result<Box<[LinuxPathCapability]>> {
    use std::collections::BTreeSet;

    const ACCESS_FS_READ_FILE: u64 = 1 << 2;
    const ACCESS_FS_READ_DIR: u64 = 1 << 3;
    let mut canonical_paths = BTreeSet::new();
    let mut capabilities = Vec::new();
    for candidate in [
        "/usr",
        "/lib",
        "/lib64",
        "/etc/ld.so.cache",
        "/dev/null",
        "/dev/urandom",
        "/dev/random",
    ] {
        let path = Path::new(candidate);
        let canonical = match dunce::canonicalize(path) {
            Ok(path) => path,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        if !canonical_paths.insert(canonical.clone()) {
            continue;
        }
        verify_linux_protected_read_path(&canonical)?;
        let file = open_linux_path_capability(&canonical)?;
        let metadata = file.metadata()?;
        let allowed_access = ACCESS_FS_READ_FILE
            | if metadata.is_dir() {
                ACCESS_FS_READ_DIR
            } else {
                0
            };
        capabilities.push(LinuxPathCapability {
            file,
            allowed_access,
        });
    }
    Ok(capabilities.into_boxed_slice())
}

#[cfg(target_os = "linux")]
fn verify_linux_protected_read_path(path: &Path) -> io::Result<()> {
    let mut current = Some(path);
    while let Some(component) = current {
        let metadata = fs::symlink_metadata(component)?;
        // Standard Linux character devices such as /dev/null and
        // /dev/urandom are intentionally mode 0666. They are still safe
        // read-only Landlock capabilities when the exact leaf is a
        // root-owned character device and every ancestor is protected. Do
        // not extend this exception to regular files or directories.
        let protected_device_leaf =
            component == path && metadata.file_type().is_char_device() && metadata.uid() == 0;
        if metadata.file_type().is_symlink()
            || metadata.uid() != 0
            || (!protected_device_leaf && metadata.permissions().mode() & 0o022 != 0)
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "Linux runtime read capability is not root-owned and protected: {}",
                    component.display()
                ),
            ));
        }
        current = component.parent().filter(|parent| *parent != component);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn apply_linux_landlock(
    output_fd: std::os::fd::RawFd,
    executable_fd: std::os::fd::RawFd,
    read_capabilities: &[LinuxPathCapability],
) -> io::Result<()> {
    use std::os::fd::AsRawFd as _;

    #[repr(C)]
    struct RulesetAttr {
        handled_access_fs: u64,
    }

    const CREATE_RULESET_VERSION: u32 = 1;
    const ACCESS_FS_EXECUTE: u64 = 1 << 0;
    const ACCESS_FS_WRITE_FILE: u64 = 1 << 1;
    const ACCESS_FS_READ_FILE: u64 = 1 << 2;
    const ACCESS_FS_READ_DIR: u64 = 1 << 3;
    const ACCESS_FS_REMOVE_DIR: u64 = 1 << 4;
    const ACCESS_FS_REMOVE_FILE: u64 = 1 << 5;
    const ACCESS_FS_MAKE_CHAR: u64 = 1 << 6;
    const ACCESS_FS_MAKE_DIR: u64 = 1 << 7;
    const ACCESS_FS_MAKE_REG: u64 = 1 << 8;
    const ACCESS_FS_MAKE_SOCK: u64 = 1 << 9;
    const ACCESS_FS_MAKE_FIFO: u64 = 1 << 10;
    const ACCESS_FS_MAKE_BLOCK: u64 = 1 << 11;
    const ACCESS_FS_MAKE_SYM: u64 = 1 << 12;
    const ACCESS_FS_REFER: u64 = 1 << 13;
    const ACCESS_FS_TRUNCATE: u64 = 1 << 14;

    let abi = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<RulesetAttr>(),
            0usize,
            CREATE_RULESET_VERSION,
        )
    };
    if abi < 0 {
        return Err(io::Error::last_os_error());
    }
    // ABI 1 and 2 cannot mediate truncation. Accepting them would let a cell
    // mutate an existing file outside the output capability with truncate(2)
    // or O_RDONLY|O_TRUNC even though WRITE_FILE was denied there.
    if abi < 3 {
        return Err(io::Error::from_raw_os_error(libc::EOPNOTSUPP));
    }

    // Handle every write-like object creation right so it is denied outside
    // the output capability. Inside that capability, allow only regular files
    // and directories: the artifact snapshot cannot represent devices,
    // sockets, FIFOs or symlinks, so the child has no reason to create them.
    const HANDLED_ACCESS: u64 = ACCESS_FS_EXECUTE
        | ACCESS_FS_WRITE_FILE
        | ACCESS_FS_READ_FILE
        | ACCESS_FS_READ_DIR
        | ACCESS_FS_REMOVE_DIR
        | ACCESS_FS_REMOVE_FILE
        | ACCESS_FS_MAKE_CHAR
        | ACCESS_FS_MAKE_DIR
        | ACCESS_FS_MAKE_REG
        | ACCESS_FS_MAKE_SOCK
        | ACCESS_FS_MAKE_FIFO
        | ACCESS_FS_MAKE_BLOCK
        | ACCESS_FS_MAKE_SYM
        | ACCESS_FS_REFER
        | ACCESS_FS_TRUNCATE;
    const ALLOWED_OUTPUT_ACCESS: u64 = ACCESS_FS_WRITE_FILE
        | ACCESS_FS_READ_FILE
        | ACCESS_FS_READ_DIR
        | ACCESS_FS_REMOVE_DIR
        | ACCESS_FS_REMOVE_FILE
        | ACCESS_FS_MAKE_DIR
        | ACCESS_FS_MAKE_REG
        | ACCESS_FS_REFER
        | ACCESS_FS_TRUNCATE;
    let ruleset_attr = RulesetAttr {
        handled_access_fs: HANDLED_ACCESS,
    };
    let ruleset_fd = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            &ruleset_attr,
            std::mem::size_of::<RulesetAttr>(),
            0u32,
        )
    };
    if ruleset_fd < 0 {
        return Err(io::Error::last_os_error());
    }
    if add_linux_landlock_path_rule(ruleset_fd, output_fd, ALLOWED_OUTPUT_ACCESS) < 0 {
        let error = io::Error::last_os_error();
        unsafe { libc::close(ruleset_fd as i32) };
        return Err(error);
    }
    // The first exec is allowed only for the exact protected executable inode
    // whose bytes were re-hashed against the sealed admission snapshot.
    // Because EXECUTE is handled but no runtime directory receives that right,
    // a cell cannot later replace its provenance with /bin/sh or another
    // binary.
    if add_linux_landlock_path_rule(
        ruleset_fd,
        executable_fd,
        ACCESS_FS_EXECUTE | ACCESS_FS_READ_FILE,
    ) < 0
    {
        let error = io::Error::last_os_error();
        unsafe { libc::close(ruleset_fd as i32) };
        return Err(error);
    }
    for capability in read_capabilities {
        if add_linux_landlock_path_rule(
            ruleset_fd,
            capability.file.as_raw_fd(),
            capability.allowed_access,
        ) < 0
        {
            let error = io::Error::last_os_error();
            unsafe { libc::close(ruleset_fd as i32) };
            return Err(error);
        }
    }
    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        let error = io::Error::last_os_error();
        unsafe { libc::close(ruleset_fd as i32) };
        return Err(error);
    }
    let restrict_result =
        unsafe { libc::syscall(libc::SYS_landlock_restrict_self, ruleset_fd, 0u32) };
    let restrict_error = (restrict_result < 0).then(io::Error::last_os_error);
    let close_result = unsafe { libc::close(ruleset_fd as i32) };
    if let Some(error) = restrict_error {
        return Err(error);
    }
    if close_result != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn add_linux_landlock_path_rule(
    ruleset_fd: libc::c_long,
    parent_fd: std::os::fd::RawFd,
    allowed_access: u64,
) -> libc::c_long {
    #[repr(C)]
    struct PathBeneathAttr {
        allowed_access: u64,
        parent_fd: i32,
    }
    const RULE_PATH_BENEATH: u32 = 1;
    let path_attr = PathBeneathAttr {
        allowed_access,
        parent_fd,
    };
    // SAFETY: all pointers refer to stack values that remain live for the
    // duration of the syscall.
    unsafe {
        libc::syscall(
            libc::SYS_landlock_add_rule,
            ruleset_fd,
            RULE_PATH_BENEATH,
            &path_attr,
            0u32,
        )
    }
}

#[cfg(target_os = "linux")]
struct LinuxSeccompFilter {
    instructions: Box<[libc::sock_filter]>,
    len: u16,
}

#[cfg(target_os = "linux")]
fn build_linux_seccomp_filter() -> io::Result<LinuxSeccompFilter> {
    const BPF_LD: u16 = 0x00;
    const BPF_W: u16 = 0x00;
    const BPF_ABS: u16 = 0x20;
    const BPF_JMP: u16 = 0x05;
    const BPF_JEQ: u16 = 0x10;
    #[cfg(target_arch = "x86_64")]
    const BPF_JSET: u16 = 0x40;
    const BPF_K: u16 = 0x00;
    const BPF_RET: u16 = 0x06;
    const RET_KILL_PROCESS: u32 = 0x8000_0000;
    const RET_ERRNO: u32 = 0x0005_0000;
    const RET_ALLOW: u32 = 0x7fff_0000;
    #[cfg(target_arch = "x86_64")]
    const AUDIT_ARCH: u32 = 0xc000_003e;
    #[cfg(target_arch = "aarch64")]
    const AUDIT_ARCH: u32 = 0xc000_00b7;
    #[cfg(target_arch = "x86_64")]
    const X32_SYSCALL_BIT: u32 = 0x4000_0000;

    let stmt = |code, k| libc::sock_filter {
        code,
        jt: 0,
        jf: 0,
        k,
    };
    let jump = |code, k, jt, jf| libc::sock_filter { code, jt, jf, k };
    let mut filters = vec![
        stmt(BPF_LD | BPF_W | BPF_ABS, 4),
        jump(BPF_JMP | BPF_JEQ | BPF_K, AUDIT_ARCH, 1, 0),
        stmt(BPF_RET | BPF_K, RET_KILL_PROCESS),
        stmt(BPF_LD | BPF_W | BPF_ABS, 0),
    ];
    #[cfg(target_arch = "x86_64")]
    {
        // x86-64 and x32 share AUDIT_ARCH_X86_64. A syscall-number denylist
        // that ignores bit 30 can be bypassed by issuing the x32 form.
        filters.push(jump(BPF_JMP | BPF_JSET | BPF_K, X32_SYSCALL_BIT, 0, 1));
        filters.push(stmt(BPF_RET | BPF_K, RET_ERRNO | libc::EPERM as u32));
    }

    // clone and clone3 remain denied wholesale. This intentionally prevents
    // subprocess creation, but it also prevents Linux scientific runtimes from
    // creating threads. A future flags-aware policy may admit CLONE_THREAD;
    // until then the runner must report this production limitation rather than
    // silently weakening process confinement.
    let denied_syscalls = [
        libc::SYS_socket,
        libc::SYS_socketpair,
        libc::SYS_clone,
        libc::SYS_clone3,
        libc::SYS_kill,
        libc::SYS_tkill,
        libc::SYS_tgkill,
        libc::SYS_pidfd_send_signal,
        libc::SYS_ptrace,
        libc::SYS_process_vm_readv,
        libc::SYS_process_vm_writev,
        libc::SYS_pidfd_open,
        libc::SYS_io_uring_setup,
        libc::SYS_io_uring_enter,
        libc::SYS_io_uring_register,
    ];
    for syscall in denied_syscalls {
        filters.push(jump(BPF_JMP | BPF_JEQ | BPF_K, syscall as u32, 0, 1));
        filters.push(stmt(BPF_RET | BPF_K, RET_ERRNO | libc::EPERM as u32));
    }
    #[cfg(target_arch = "x86_64")]
    for syscall in [libc::SYS_fork, libc::SYS_vfork] {
        filters.push(jump(BPF_JMP | BPF_JEQ | BPF_K, syscall as u32, 0, 1));
        filters.push(stmt(BPF_RET | BPF_K, RET_ERRNO | libc::EPERM as u32));
    }
    filters.push(stmt(BPF_RET | BPF_K, RET_ALLOW));
    let len = u16::try_from(filters.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "seccomp filter is too large"))?;
    Ok(LinuxSeccompFilter {
        instructions: filters.into_boxed_slice(),
        len,
    })
}

#[cfg(target_os = "linux")]
fn apply_linux_seccomp(filter: &LinuxSeccompFilter) -> io::Result<()> {
    let mut program = libc::sock_fprog {
        len: filter.len,
        filter: filter.instructions.as_ptr().cast_mut(),
    };
    if unsafe {
        libc::prctl(
            libc::PR_SET_SECCOMP,
            libc::SECCOMP_MODE_FILTER,
            &mut program as *mut libc::sock_fprog,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn duplicate_inheritable(snapshot: &File) -> Result<File, PinExecutableError> {
    let inherited_fd = unsafe {
        // SAFETY: `snapshot` is a live descriptor and `F_DUPFD` duplicates it
        // without taking ownership of the original.
        libc::fcntl(snapshot.as_raw_fd(), libc::F_DUPFD, 3)
    };
    if inherited_fd < 0 {
        return Err(PinExecutableError::CommandSetup {
            source: io::Error::last_os_error(),
        });
    }
    // SAFETY: `F_DUPFD` returned a new descriptor owned by this value.
    Ok(unsafe { File::from_raw_fd(inherited_fd) })
}

#[cfg(target_os = "linux")]
fn copy_and_hash(source: &mut File, destination: &mut File) -> io::Result<String> {
    source.seek(SeekFrom::Start(0))?;
    destination.seek(SeekFrom::Start(0))?;
    destination.set_len(0)?;

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        destination.write_all(&buffer[..read])?;
    }
    destination.flush()?;
    destination.sync_all()?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn hash_file(file: &mut File) -> io::Result<String> {
    file.seek(SeekFrom::Start(0))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(target_os = "linux")]
fn create_snapshot(
    source: &mut File,
    executable_mode: u32,
) -> Result<(PinnedSnapshot, String), PinExecutableError> {
    let name = c"lumen-pinned-executable";
    let raw_fd = unsafe {
        // SAFETY: `name` is a valid C string and the returned descriptor is
        // checked before ownership is transferred.
        libc::syscall(
            libc::SYS_memfd_create,
            name.as_ptr(),
            libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
        )
    };
    if raw_fd < 0 {
        return Err(PinExecutableError::SnapshotIo {
            operation: "create sealed memfd snapshot",
            source: io::Error::last_os_error(),
        });
    }
    let mut snapshot = unsafe {
        // SAFETY: successful `memfd_create` returned a new descriptor.
        File::from_raw_fd(raw_fd as libc::c_int)
    };
    let copied_sha256 =
        copy_and_hash(source, &mut snapshot).map_err(|source| PinExecutableError::SnapshotIo {
            operation: "copy executable into memfd",
            source,
        })?;
    snapshot
        .set_permissions(fs::Permissions::from_mode(executable_mode))
        .map_err(|source| PinExecutableError::SnapshotIo {
            operation: "set memfd execute permissions",
            source,
        })?;
    snapshot
        .sync_all()
        .map_err(|source| PinExecutableError::SnapshotIo {
            operation: "fsync memfd snapshot",
            source,
        })?;
    let snapshot_sha256 =
        hash_file(&mut snapshot).map_err(|source| PinExecutableError::SnapshotIo {
            operation: "re-hash memfd snapshot",
            source,
        })?;
    if copied_sha256 != snapshot_sha256 {
        return Err(PinExecutableError::SnapshotDigestMismatch {
            copied_sha256,
            snapshot_sha256,
        });
    }

    let required_seals =
        libc::F_SEAL_SEAL | libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_WRITE;
    let seal_result = unsafe {
        // SAFETY: `snapshot` owns a valid memfd and `F_ADD_SEALS` takes an
        // integer bit mask.
        libc::fcntl(snapshot.as_raw_fd(), libc::F_ADD_SEALS, required_seals)
    };
    if seal_result < 0 {
        return Err(PinExecutableError::SnapshotSealingFailed {
            detail: io::Error::last_os_error().to_string(),
        });
    }
    let observed_seals = unsafe {
        // SAFETY: `snapshot` owns a valid memfd; `F_GET_SEALS` has no third
        // argument.
        libc::fcntl(snapshot.as_raw_fd(), libc::F_GET_SEALS)
    };
    if observed_seals < 0 || observed_seals & required_seals != required_seals {
        return Err(PinExecutableError::SnapshotSealingFailed {
            detail: if observed_seals < 0 {
                io::Error::last_os_error().to_string()
            } else {
                format!("required seals {required_seals:#x}, observed {observed_seals:#x}")
            },
        });
    }

    Ok((
        PinnedSnapshot {
            file: snapshot,
            backend: PinnedExecutableBackend::LinuxSealedMemfd,
        },
        copied_sha256,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[derive(Debug)]
    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos();
            let counter = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "lumen-pinned-executable-test-{}-{nonce}-{counter}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create isolated test directory");
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_executable(path: &Path, bytes: &[u8]) {
        fs::write(path, bytes).expect("write executable fixture");
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .expect("set executable fixture mode");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn replacement_at_original_path_cannot_change_executed_bytes() {
        let dir = TestDir::new();
        let executable = dir.0.join("kernel");
        let original = b"#!/bin/sh\nprintf 'pinned-bytes\\n'\n";
        write_executable(&executable, original);

        let pinned = PinnedExecutable::pin(&executable).expect("pin original executable");
        let canonical_display = dunce::canonicalize(&executable).expect("canonical fixture");
        assert_eq!(pinned.canonical_path(), canonical_display);
        assert_eq!(pinned.sha256(), format!("{:x}", Sha256::digest(original)));

        fs::rename(&executable, dir.0.join("replaced-original"))
            .expect("move original path object");
        write_executable(&executable, b"#!/bin/sh\nprintf 'replacement-bytes\\n'\n");

        let mut command = pinned.spawn_command().expect("build pinned command");
        let output = command.output().expect("execute pinned snapshot");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "pinned-bytes\n");

        let replacement_output = Command::new(&executable)
            .output()
            .expect("execute replacement as a control");
        assert_eq!(
            String::from_utf8_lossy(&replacement_output.stdout),
            "replacement-bytes\n"
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn linux_runtime_read_capabilities_accept_standard_character_devices() {
        let capabilities = open_linux_runtime_read_capabilities()
            .expect("root-owned standard devices may be retained read-only");
        assert!(
            !capabilities.is_empty(),
            "the Linux runtime capability set is unexpectedly empty"
        );
        for capability in &capabilities {
            let status_flags = unsafe {
                // SAFETY: the capability owns a live descriptor and F_GETFL
                // only reads its open-file status flags.
                libc::fcntl(capability.file.as_raw_fd(), libc::F_GETFL)
            };
            assert!(status_flags >= 0, "read Landlock capability flags");
            assert_ne!(
                status_flags & libc::O_PATH,
                0,
                "Landlock path-beneath rules require O_PATH descriptors"
            );
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn linux_landlock_uses_path_only_views_of_output_and_protected_executable() {
        let dir = TestDir::new();
        let output = File::open(&dir.0).expect("open retained output directory");
        let output_capability =
            open_linux_directory_path_capability(output.as_raw_fd()).expect("retain output O_PATH");

        let executable = dunce::canonicalize("/bin/sh").expect("canonical /bin/sh");
        let pinned = PinnedExecutable::pin(&executable).expect("pin executable into sealed memfd");
        let executable_capability =
            open_linux_path_capability(&executable).expect("retain protected executable O_PATH");
        let verified = open_verified_linux_executable_capability(&executable, pinned.sha256())
            .expect("protected executable still matches sealed snapshot");
        let verified_metadata = verified.metadata().expect("stat verified executable");
        let executable_metadata = executable_capability
            .metadata()
            .expect("stat executable capability");
        assert_eq!(
            (verified_metadata.dev(), verified_metadata.ino()),
            (executable_metadata.dev(), executable_metadata.ino())
        );

        for capability in [&output_capability, &executable_capability] {
            let status_flags = unsafe {
                // SAFETY: each capability owns a live descriptor and F_GETFL
                // only reads its open-file status flags.
                libc::fcntl(capability.as_raw_fd(), libc::F_GETFL)
            };
            assert!(status_flags >= 0, "read retained capability flags");
            assert_ne!(
                status_flags & libc::O_PATH,
                0,
                "Landlock path-beneath rules require O_PATH descriptors"
            );
        }
        let source = fs::metadata(&executable).expect("stat protected executable");
        let retained = executable_capability
            .metadata()
            .expect("stat retained executable path");
        assert_eq!(
            (source.dev(), source.ino()),
            (retained.dev(), retained.ino())
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn linux_sandboxed_protected_executable_starts_without_inheriting_memfd() {
        let dir = TestDir::new();
        let output = File::open(&dir.0).expect("open retained output directory");
        let executable = dunce::canonicalize("/bin/sh").expect("canonical /bin/sh");
        let pinned = PinnedExecutable::pin(&executable).expect("pin protected shell");
        let mut command = pinned
            .spawn_linux_sandboxed_command(output.as_raw_fd())
            .expect("build Landlock command from protected executable");
        assert!(
            command.inherited_snapshot.is_none(),
            "sandboxed child must not inherit the sealed admission memfd"
        );
        command
            .command_mut()
            .args(["-c", "printf 'landlock-protected-path\\n'"]);
        let result = command.output().expect("run protected shell in Landlock");
        assert!(result.status.success(), "{result:?}");
        assert_eq!(
            String::from_utf8_lossy(&result.stdout),
            "landlock-protected-path\n"
        );
    }

    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn pinned_system_python_runs_version_and_code() {
        #[cfg(target_os = "macos")]
        let python = PathBuf::from(
            "/Library/Developer/CommandLineTools/Library/Frameworks/\
             Python3.framework/Versions/3.9/Resources/Python.app/Contents/MacOS/Python",
        );
        #[cfg(target_os = "linux")]
        let python = {
            let discovered = Command::new("/bin/sh")
                .args(["-c", "command -v python3"])
                .output()
                .expect("discover python3");
            assert!(discovered.status.success(), "{discovered:?}");
            PathBuf::from(
                String::from_utf8(discovered.stdout)
                    .expect("python3 path is UTF-8")
                    .trim(),
            )
        };
        let pinned = PinnedExecutable::pin(&python).expect("pin discovered Python");

        let mut version = pinned.spawn_command().expect("build version command");
        version.command_mut().arg("--version");
        let version_output = version.output().expect("run Python --version");
        assert!(version_output.status.success(), "{version_output:?}");
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&version_output.stdout),
            String::from_utf8_lossy(&version_output.stderr)
        );
        assert!(combined.starts_with("Python "), "{combined:?}");

        let mut code = pinned.spawn_command().expect("build code command");
        code.command_mut()
            .args(["-c", "print('pinned-python-code')"]);
        let code_output = code.output().expect("run pinned Python code");
        assert!(code_output.status.success(), "{code_output:?}");
        assert_eq!(
            String::from_utf8_lossy(&code_output.stdout),
            "pinned-python-code\n"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_rejects_a_user_writable_executable_path() {
        let dir = TestDir::new();
        let executable = dir.0.join("kernel");
        write_executable(&executable, b"#!/bin/sh\nexit 0\n");
        assert!(matches!(
            PinnedExecutable::pin(&executable),
            Err(PinExecutableError::MacOsPathNotProtected { .. })
        ));
    }

    #[test]
    fn relative_paths_are_rejected_before_platform_dispatch() {
        let result = PinnedExecutable::pin(Path::new("python3"));
        assert!(matches!(
            result,
            Err(PinExecutableError::PathNotAbsolute { .. })
        ));
    }
}
