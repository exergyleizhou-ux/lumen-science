//! Retained filesystem capabilities for approval-gated workflow execution.
//!
//! Preparing a workflow opens the already-provisioned store root and retains
//! that exact directory. It does not create `workflow-cells` or
//! `workflow-outputs`. Methods that mutate those trees are deliberately
//! separate so only the post-Allow path can call them.

use crate::project::capability::PinnedDirectory;
use crate::{Result, ScienceError};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
#[cfg(unix)]
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const CELL_DIRECTORY: &str = "workflow-cells";
const OUTPUT_DIRECTORY: &str = "workflow-outputs";
const STDOUT_FILE: &str = "stdout.txt";
const STDERR_FILE: &str = "stderr.txt";

/// The already-open workflow store root.
///
/// Cloning this value clones an `Arc`, not a pathname. Every descendant is
/// opened relative to the retained [`PinnedDirectory`].
#[derive(Debug, Clone)]
pub struct WorkflowIoCapability {
    root: Arc<PinnedDirectory>,
}

impl WorkflowIoCapability {
    /// Retain an existing store root proven to be below `workspace`.
    ///
    /// This is the prepare-phase constructor. It never creates `store_root`
    /// or a workflow child directory.
    pub fn open_existing_confined(store_root: &Path, workspace: &Path) -> Result<Self> {
        Ok(Self {
            root: Arc::new(PinnedDirectory::open_existing_within(
                store_root, workspace,
            )?),
        })
    }

    /// Share the exact retained root with another post-approval component.
    pub fn share(&self) -> Self {
        self.clone()
    }

    /// Share the retained root with crate-internal workflow components.
    pub(crate) fn shared_root(&self) -> Arc<PinnedDirectory> {
        Arc::clone(&self.root)
    }

    /// Stage a content-addressed cell after approval.
    ///
    /// The caller-provided digest must match `source`. An existing identical
    /// cell is an idempotent success; an existing different cell fails closed.
    pub fn stage_cell(&self, sha256: &str, source: &[u8]) -> Result<()> {
        validate_sha256(sha256)?;
        let actual = hex_sha256(source);
        if actual != sha256 {
            return Err(ScienceError::Invalid(format!(
                "workflow cell source hashes to {actual}, not requested digest {sha256}"
            )));
        }
        let relative = PathBuf::from(CELL_DIRECTORY).join(sha256);
        match self.root.write_new_atomic(&relative, source) {
            Ok(()) => Ok(()),
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                match self.root.read_optional(&relative)? {
                    Some(existing) if existing == source => Ok(()),
                    Some(_) => Err(ScienceError::Invalid(format!(
                        "workflow cell {sha256} already exists with different bytes"
                    ))),
                    None => Err(ScienceError::Invalid(format!(
                        "workflow cell {sha256} disappeared during idempotent staging"
                    ))),
                }
            }
            Err(error) => Err(error),
        }
    }

    /// Read and re-verify a content-addressed cell relative to the retained
    /// root. A corrupt file is an error, never a cache miss.
    pub fn read_cell(&self, sha256: &str) -> Result<Option<Vec<u8>>> {
        validate_sha256(sha256)?;
        let relative = PathBuf::from(CELL_DIRECTORY).join(sha256);
        let Some(bytes) = self.root.read_optional(&relative)? else {
            return Ok(None);
        };
        let actual = hex_sha256(&bytes);
        if actual != sha256 {
            return Err(ScienceError::Invalid(format!(
                "workflow cell {sha256} contains bytes hashing to {actual}"
            )));
        }
        Ok(Some(bytes))
    }

    /// Create and retain the per-attempt output directory after approval.
    ///
    /// The retained child remains authoritative even if an attacker later
    /// renames the store root and installs a symlink at its former pathname.
    pub fn create_attempt_output(
        &self,
        run_id: &str,
        step_id: &str,
        attempt_id: &str,
    ) -> Result<AttemptOutputCapability> {
        validate_component(run_id, "workflow run id")?;
        validate_component(step_id, "workflow step id")?;
        validate_component(attempt_id, "workflow attempt id")?;
        let relative = PathBuf::from(OUTPUT_DIRECTORY)
            .join(run_id)
            .join(step_id)
            .join(attempt_id);
        Ok(AttemptOutputCapability {
            directory: Arc::new(self.root.create_directory(&relative)?),
        })
    }
}

/// A retained per-attempt output directory.
///
/// All writes and snapshots are descriptor-relative. There is intentionally no
/// absolute-path getter: reopening a path would discard the authority this
/// object carries.
#[derive(Debug, Clone)]
pub struct AttemptOutputCapability {
    directory: Arc<PinnedDirectory>,
}

impl AttemptOutputCapability {
    /// Share the exact retained attempt directory.
    pub fn share(&self) -> Self {
        self.clone()
    }

    /// Atomically replace one regular output file.
    pub fn write_atomic(&self, relative: &Path, bytes: &[u8]) -> Result<()> {
        self.directory.replace_atomic(relative, bytes)
    }

    /// Read one regular output file without following symlinks.
    pub fn read(&self, relative: &Path) -> Result<Option<Vec<u8>>> {
        self.directory.read_optional(relative)
    }

    /// Atomically publish captured standard output.
    pub fn write_stdout(&self, bytes: &[u8]) -> Result<()> {
        self.write_atomic(Path::new(STDOUT_FILE), bytes)
    }

    /// Atomically publish captured standard error.
    pub fn write_stderr(&self, bytes: &[u8]) -> Result<()> {
        self.write_atomic(Path::new(STDERR_FILE), bytes)
    }

    /// Create and retain an output descendant, for example `figures`.
    pub fn create_directory(&self, relative: &Path) -> Result<RetainedOutputDirectory> {
        Ok(RetainedOutputDirectory {
            directory: Arc::new(self.directory.create_directory(relative)?),
        })
    }

    /// Retain the attempt directory for an approved child.
    ///
    /// The child is moved into this exact directory with `fchdir` immediately
    /// before exec. Output paths are then relative (`.` and `figures`), so
    /// macOS never has to downgrade the descriptor into an `F_GETPATH`
    /// pathname that an attacker could swap before spawn.
    #[cfg(unix)]
    pub fn child_paths(&self) -> Result<WorkflowChildPaths> {
        self.directory.create_directory(Path::new("figures"))?;
        let output_fd = self.directory.duplicate_inheritable()?;
        Ok(WorkflowChildPaths {
            output_path: PathBuf::from("."),
            figures_path: PathBuf::from("figures"),
            #[cfg(target_os = "macos")]
            sandbox_root: macos_descriptor_path(&output_fd)?,
            _output_fd: output_fd,
        })
    }

    /// Windows needs an inherited handle protocol that retains every ancestor
    /// and reconstructs a verified child-visible path. A pathname fallback
    /// would reintroduce the swap vulnerability, so it fails closed.
    #[cfg(not(unix))]
    pub fn child_paths(&self) -> Result<WorkflowChildPaths> {
        Err(ScienceError::FeatureDisabled(
            "retained workflow child paths are unavailable on this platform".into(),
        ))
    }

    /// Hash every regular output file without following symlinks.
    ///
    /// Encountering a symlink, reparse point, FIFO, device, or socket rejects
    /// the entire snapshot. Such entries are never silently omitted.
    pub fn snapshot(&self) -> Result<WorkflowOutputSnapshot> {
        snapshot(&self.directory)
    }

    /// Hash output while bounding aggregate bytes buffered by the parent.
    pub fn snapshot_bounded(&self, max_bytes: u64) -> Result<WorkflowOutputSnapshot> {
        snapshot_bounded(&self.directory, max_bytes)
    }
}

/// A retained descendant of an attempt output directory.
#[derive(Debug, Clone)]
pub struct RetainedOutputDirectory {
    directory: Arc<PinnedDirectory>,
}

/// Child-visible relative paths backed by a retained current-directory handle.
///
/// Dropping this value closes the retained descriptor. Keep it alive until
/// after `Child::wait`.
#[derive(Debug)]
pub struct WorkflowChildPaths {
    output_path: PathBuf,
    figures_path: PathBuf,
    #[cfg(target_os = "macos")]
    sandbox_root: PathBuf,
    #[cfg(unix)]
    _output_fd: fs::File,
}

impl WorkflowChildPaths {
    pub fn output_path(&self) -> &Path {
        &self.output_path
    }

    pub fn figures_path(&self) -> &Path {
        &self.figures_path
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sandbox_root(&self) -> &Path {
        &self.sandbox_root
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn output_fd(&self) -> std::os::fd::RawFd {
        use std::os::fd::AsRawFd as _;
        self._output_fd.as_raw_fd()
    }

    /// Make the child enter the retained attempt directory immediately before
    /// exec, then close the descriptor on exec.
    #[cfg(unix)]
    pub fn configure_command(&self, command: &mut std::process::Command) {
        use std::os::fd::AsRawFd as _;
        use std::os::unix::process::CommandExt as _;

        let output_fd = self._output_fd.as_raw_fd();
        // SAFETY: the descriptor remains owned by `self` until the child has
        // spawned. The closure uses only async-signal-safe libc operations.
        unsafe {
            command.pre_exec(move || {
                if libc::fchdir(output_fd) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::fcntl(output_fd, libc::F_SETFD, libc::FD_CLOEXEC) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    #[cfg(not(unix))]
    pub fn configure_command(&self, _command: &mut std::process::Command) {}
}

#[cfg(target_os = "macos")]
fn macos_descriptor_path(file: &fs::File) -> Result<PathBuf> {
    use std::os::fd::AsRawFd as _;
    use std::os::unix::ffi::OsStringExt as _;

    let mut buffer = vec![0_i8; libc::PATH_MAX as usize];
    // SAFETY: F_GETPATH writes at most PATH_MAX bytes for the live descriptor.
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETPATH, buffer.as_mut_ptr()) } < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: successful F_GETPATH returns a NUL-terminated path.
    let path = unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) };
    Ok(PathBuf::from(std::ffi::OsString::from_vec(
        path.to_bytes().to_vec(),
    )))
}

impl RetainedOutputDirectory {
    pub fn share(&self) -> Self {
        self.clone()
    }

    pub fn write_atomic(&self, relative: &Path, bytes: &[u8]) -> Result<()> {
        self.directory.replace_atomic(relative, bytes)
    }

    pub fn read(&self, relative: &Path) -> Result<Option<Vec<u8>>> {
        self.directory.read_optional(relative)
    }

    pub fn create_directory(&self, relative: &Path) -> Result<Self> {
        Ok(Self {
            directory: Arc::new(self.directory.create_directory(relative)?),
        })
    }

    pub fn snapshot(&self) -> Result<WorkflowOutputSnapshot> {
        snapshot(&self.directory)
    }

    pub fn snapshot_bounded(&self, max_bytes: u64) -> Result<WorkflowOutputSnapshot> {
        snapshot_bounded(&self.directory, max_bytes)
    }
}

/// Deterministic artifact manifest produced from one retained directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowOutputSnapshot {
    /// Slash-separated relative file name to lowercase SHA-256.
    pub artifacts: BTreeMap<String, String>,
    /// The exact bytes behind `artifacts`.
    ///
    /// The workflow executor, not the runner, is the authority that hashes
    /// and publishes these bytes into its immutable artifact store. Keeping
    /// the bytes in the retained-directory snapshot prevents a runner from
    /// claiming a digest for content the executor never observed.
    pub artifact_bytes: BTreeMap<String, Vec<u8>>,
    pub bytes_produced: u64,
}

fn snapshot(directory: &PinnedDirectory) -> Result<WorkflowOutputSnapshot> {
    snapshot_bounded(directory, u64::MAX)
}

fn snapshot_bounded(directory: &PinnedDirectory, max_bytes: u64) -> Result<WorkflowOutputSnapshot> {
    let mut artifacts = BTreeMap::new();
    let mut artifact_bytes = BTreeMap::new();
    let mut bytes_produced = 0u64;
    for (relative, bytes) in directory.snapshot_nofollow_bounded(max_bytes)? {
        let name = snapshot_name(&relative)?;
        bytes_produced = bytes_produced
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| {
                ScienceError::Invalid("workflow output snapshot byte count overflowed".into())
            })?;
        artifacts.insert(name.clone(), hex_sha256(&bytes));
        artifact_bytes.insert(name, bytes);
    }
    Ok(WorkflowOutputSnapshot {
        artifacts,
        artifact_bytes,
        bytes_produced,
    })
}

fn snapshot_name(relative: &Path) -> Result<String> {
    let name = relative
        .to_str()
        .ok_or_else(|| ScienceError::Invalid("workflow output path must be valid UTF-8".into()))?;
    Ok(name.replace('\\', "/"))
}

fn validate_sha256(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ScienceError::Invalid(
            "workflow cell digest must be exactly 64 lowercase hex characters".into(),
        ));
    }
    Ok(())
}

fn validate_component(value: &str, field: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || matches!(value, "." | "..")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(ScienceError::Invalid(format!(
            "{field} must be 1..=128 [A-Za-z0-9._-] characters and not dot traversal"
        )));
    }
    Ok(())
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn prepare_opens_existing_root_without_creating_workflow_children() {
        let workspace = tempdir().unwrap();
        let store = workspace.path().join("science");
        fs::create_dir(&store).unwrap();

        let _io = WorkflowIoCapability::open_existing_confined(&store, workspace.path()).unwrap();

        assert!(!store.join(CELL_DIRECTORY).exists());
        assert!(!store.join(OUTPUT_DIRECTORY).exists());
    }

    #[test]
    fn missing_store_root_is_not_created_during_prepare() {
        let workspace = tempdir().unwrap();
        let store = workspace.path().join("missing-science");

        assert!(WorkflowIoCapability::open_existing_confined(&store, workspace.path()).is_err());
        assert!(!store.exists());
    }

    #[cfg(unix)]
    #[test]
    fn retained_root_survives_rename_and_symlink_without_writing_outside() {
        use std::os::unix::fs::symlink;
        use std::process::Command;

        let workspace = tempdir().unwrap();
        let store = workspace.path().join("science");
        let retained = workspace.path().join("retained-science");
        let outside = workspace.path().join("outside");
        fs::create_dir(&store).unwrap();
        fs::create_dir(&outside).unwrap();
        let io = WorkflowIoCapability::open_existing_confined(&store, workspace.path()).unwrap();

        fs::rename(&store, &retained).unwrap();
        symlink(&outside, &store).unwrap();

        let source = b"print('retained')";
        let digest = hex_sha256(source);
        io.stage_cell(&digest, source).unwrap();
        let attempt = io
            .create_attempt_output("run-1", "step-1", "attempt-1")
            .unwrap();
        attempt.write_stdout(b"inside only").unwrap();
        attempt.write_stderr(b"").unwrap();
        let child_paths = attempt.child_paths().unwrap();
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(
                "printf 'child-output' > \"$LUMEN_KERNEL_OUTPUT_DIR/child.txt\"; \
                 printf 'figure-output' > \"$LUMEN_KERNEL_FIGURES_DIR/figure.txt\"",
            )
            .env("LUMEN_KERNEL_OUTPUT_DIR", child_paths.output_path())
            .env("LUMEN_KERNEL_FIGURES_DIR", child_paths.figures_path());
        child_paths.configure_command(&mut command);
        let child = command.status().unwrap();
        assert!(child.success());

        assert_eq!(
            fs::read(retained.join(CELL_DIRECTORY).join(&digest)).unwrap(),
            source
        );
        assert_eq!(io.read_cell(&digest).unwrap(), Some(source.to_vec()));
        assert_eq!(
            fs::read(
                retained
                    .join(OUTPUT_DIRECTORY)
                    .join("run-1/step-1/attempt-1/stdout.txt")
            )
            .unwrap(),
            b"inside only"
        );
        let snapshot = attempt.snapshot().unwrap();
        assert_eq!(
            snapshot.artifacts.get(STDOUT_FILE),
            Some(&hex_sha256(b"inside only"))
        );
        assert_eq!(snapshot.artifacts.get(STDERR_FILE), Some(&hex_sha256(b"")));
        assert_eq!(
            snapshot.artifacts.get("child.txt"),
            Some(&hex_sha256(b"child-output"))
        );
        assert_eq!(
            snapshot.artifacts.get("figures/figure.txt"),
            Some(&hex_sha256(b"figure-output"))
        );
        assert!(
            fs::read_dir(&outside).unwrap().next().is_none(),
            "retained capability must write zero bytes through replacement symlink"
        );
    }

    #[cfg(unix)]
    #[test]
    fn child_fchdir_survives_attempt_path_swap_after_paths_are_prepared() {
        use std::os::unix::fs::symlink;
        use std::process::Command;

        let workspace = tempdir().unwrap();
        let store = workspace.path().join("science");
        let outside = workspace.path().join("outside");
        fs::create_dir(&store).unwrap();
        fs::create_dir(&outside).unwrap();
        let io = WorkflowIoCapability::open_existing_confined(&store, workspace.path()).unwrap();
        let attempt = io
            .create_attempt_output("run-1", "step-1", "attempt-1")
            .unwrap();
        let child_paths = attempt.child_paths().unwrap();
        let attempt_path = store.join(OUTPUT_DIRECTORY).join("run-1/step-1/attempt-1");
        let retained_attempt = store
            .join(OUTPUT_DIRECTORY)
            .join("run-1/step-1/retained-attempt");
        fs::rename(&attempt_path, &retained_attempt).unwrap();
        symlink(&outside, &attempt_path).unwrap();

        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "printf retained > ./post-path-swap.txt"])
            .env("LUMEN_KERNEL_OUTPUT_DIR", child_paths.output_path());
        child_paths.configure_command(&mut command);
        assert!(command.status().unwrap().success());

        assert_eq!(
            fs::read(retained_attempt.join("post-path-swap.txt")).unwrap(),
            b"retained"
        );
        assert!(
            fs::read_dir(&outside).unwrap().next().is_none(),
            "the replacement attempt symlink must receive zero bytes"
        );
    }

    #[cfg(unix)]
    #[test]
    fn child_symlink_is_rejected_without_writing_outside() {
        use std::os::unix::fs::symlink;

        let workspace = tempdir().unwrap();
        let store = workspace.path().join("science");
        let outside = workspace.path().join("outside");
        fs::create_dir(&store).unwrap();
        fs::create_dir(&outside).unwrap();
        let io = WorkflowIoCapability::open_existing_confined(&store, workspace.path()).unwrap();
        symlink(&outside, store.join(CELL_DIRECTORY)).unwrap();

        let source = b"print('must not escape')";
        let digest = hex_sha256(source);
        assert!(io.stage_cell(&digest, source).is_err());
        assert!(
            fs::read_dir(&outside).unwrap().next().is_none(),
            "child symlink must receive zero bytes"
        );
    }

    #[cfg(unix)]
    #[test]
    fn snapshot_rejects_symlink_and_never_follows_outside_target() {
        use std::os::unix::fs::symlink;

        let workspace = tempdir().unwrap();
        let store = workspace.path().join("science");
        let outside = workspace.path().join("outside-secret.txt");
        fs::create_dir(&store).unwrap();
        fs::write(&outside, b"outside-secret-that-must-not-be-read").unwrap();
        let io = WorkflowIoCapability::open_existing_confined(&store, workspace.path()).unwrap();
        let attempt = io
            .create_attempt_output("run-1", "step-1", "attempt-1")
            .unwrap();
        attempt
            .write_atomic(Path::new("inside.txt"), b"inside")
            .unwrap();

        let attempt_path = store.join(OUTPUT_DIRECTORY).join("run-1/step-1/attempt-1");
        symlink(&outside, attempt_path.join("escape.txt")).unwrap();

        let error = attempt.snapshot().unwrap_err().to_string();
        assert!(
            error.contains("symlink"),
            "snapshot must identify the fail-closed symlink refusal: {error}"
        );
        assert_eq!(
            fs::read(&outside).unwrap(),
            b"outside-secret-that-must-not-be-read"
        );
    }

    #[test]
    fn bounded_snapshot_refuses_aggregate_output_without_buffering_the_tree() {
        let workspace = tempdir().unwrap();
        let store = workspace.path().join("science");
        fs::create_dir(&store).unwrap();
        let io = WorkflowIoCapability::open_existing_confined(&store, workspace.path()).unwrap();
        let attempt = io
            .create_attempt_output("run-1", "step-1", "attempt-1")
            .unwrap();
        attempt
            .write_atomic(Path::new("first.bin"), b"123456")
            .unwrap();
        attempt
            .write_atomic(Path::new("second.bin"), b"abcdef")
            .unwrap();

        let error = attempt.snapshot_bounded(10).unwrap_err().to_string();
        assert!(
            error.contains("10 byte cap"),
            "aggregate cap refusal must be explicit: {error}"
        );
        let exact = attempt.snapshot_bounded(12).unwrap();
        assert_eq!(exact.bytes_produced, 12);
        assert_eq!(exact.artifacts.len(), 2);
    }
}
