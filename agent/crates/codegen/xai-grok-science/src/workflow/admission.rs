//! Kernel admission by environment probe (LS5-K1).
//!
//! ## The defect this replaces
//!
//! `ProjectStore::check_kernel_admission` used to build a [`KernelAdmission`]
//! without looking at a kernel at all. It wrote `exact_version: "unknown"`,
//! hardcoded `admitted_at: "2026-07-26"` and `admitted_by:
//! "lumen-science-wp5"`, echoed the caller's `exec_hash`/`lock_hash` straight
//! back into the record as though they had been checked, and returned
//! `AdmissionStatus::Admitted` unconditionally — a function that had no path
//! to a rejection is not an admission check. Because it is reachable over ACP
//! as `x.ai/science/kernel_admission`, the product answered "admitted" for
//! interpreters that did not exist on the machine.
//!
//! Every field of a record built here is derived from the running machine:
//! the interpreter is resolved to an absolute path, the executable bytes are
//! hashed, the version comes from executing the interpreter under a timeout,
//! and OS/arch are recorded. Caller-supplied digests are *verified against the
//! probe* and are never copied into the record.

use super::kernel::{AdmissionStatus, KernelAdmission, KernelKind, ResourceCap};
use super::pinned_executable::PinnedExecutable;
use crate::{Result, ScienceError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fmt, fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

/// Largest version-probe output kept. A hostile or broken interpreter must not
/// be able to stream unbounded bytes into an admission record.
const MAX_PROBE_OUTPUT_BYTES: usize = 64 * 1024;

/// Longest version string retained in the record.
const MAX_VERSION_CHARS: usize = 256;

/// Default wall-clock budget for the version probe.
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

// ── Rejection ─────────────────────────────────────────────────────

/// Why a kernel was refused admission.
///
/// Every variant is something that was *observed*, which is why the
/// admission result carries one instead of a bare boolean.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum RejectionReason {
    /// A relative or bare-name interpreter would be resolved through `PATH`,
    /// which is not a pinned identity: the same spelling can name a different
    /// binary in another process.
    InterpreterPathNotAbsolute {
        path: String,
    },
    InterpreterNotFound {
        path: String,
    },
    InterpreterNotAFile {
        path: String,
        file_type: String,
    },
    InterpreterNotExecutable {
        path: String,
    },
    /// The path resolved (through symlinks) outside the root the caller
    /// pinned the kernel to.
    InterpreterOutsideAllowedRoot {
        resolved: String,
        allowed_root: String,
    },
    /// The caller asserted a digest that is not 64 lowercase hex characters,
    /// optionally `sha256:`-prefixed. Nothing can be verified against it.
    SuppliedDigestMalformed {
        field: String,
        value: String,
    },
    ExecutableHashMismatch {
        supplied: String,
        probed: String,
    },
    /// The bytes changed after the executable was hashed but before the
    /// admission record could be committed. The version and digest therefore
    /// do not describe one stable identity.
    InterpreterChangedDuringProbe {
        before: String,
        after: String,
    },
    PackageLockHashMismatch {
        supplied: String,
        probed: String,
    },
    /// The dependency lock changed while the interpreter probe was running.
    /// Do not bind a version observation to a different package set.
    PackageLockChangedDuringProbe {
        before: String,
        after: String,
    },
    /// A package-lock digest was asserted with no lock file to hash, so the
    /// assertion cannot be checked. Fail closed rather than accept it.
    PackageLockUnverifiable {
        supplied: String,
    },
    PackageLockNotAFile {
        path: String,
    },
    VersionProbeSpawnFailed {
        detail: String,
    },
    VersionProbeTimedOut {
        timeout_ms: u64,
    },
    VersionProbeExitNonZero {
        exit_code: Option<i32>,
        output_excerpt: String,
    },
    VersionProbeEmptyOutput,
    /// Argv that could carry a NUL or a line break is refused before spawn.
    VersionProbeArgsInvalid {
        detail: String,
    },
}

impl RejectionReason {
    /// Stable machine-readable discriminant, for metrics and assertions.
    pub fn code(&self) -> &'static str {
        match self {
            Self::InterpreterPathNotAbsolute { .. } => "interpreter_path_not_absolute",
            Self::InterpreterNotFound { .. } => "interpreter_not_found",
            Self::InterpreterNotAFile { .. } => "interpreter_not_a_file",
            Self::InterpreterNotExecutable { .. } => "interpreter_not_executable",
            Self::InterpreterOutsideAllowedRoot { .. } => "interpreter_outside_allowed_root",
            Self::SuppliedDigestMalformed { .. } => "supplied_digest_malformed",
            Self::ExecutableHashMismatch { .. } => "executable_hash_mismatch",
            Self::InterpreterChangedDuringProbe { .. } => "interpreter_changed_during_probe",
            Self::PackageLockHashMismatch { .. } => "package_lock_hash_mismatch",
            Self::PackageLockChangedDuringProbe { .. } => "package_lock_changed_during_probe",
            Self::PackageLockUnverifiable { .. } => "package_lock_unverifiable",
            Self::PackageLockNotAFile { .. } => "package_lock_not_a_file",
            Self::VersionProbeSpawnFailed { .. } => "version_probe_spawn_failed",
            Self::VersionProbeTimedOut { .. } => "version_probe_timed_out",
            Self::VersionProbeExitNonZero { .. } => "version_probe_exit_non_zero",
            Self::VersionProbeEmptyOutput => "version_probe_empty_output",
            Self::VersionProbeArgsInvalid { .. } => "version_probe_args_invalid",
        }
    }
}

impl fmt::Display for RejectionReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InterpreterPathNotAbsolute { path } => write!(
                f,
                "interpreter path '{path}' is not absolute; PATH-relative interpreters are not a pinned identity"
            ),
            Self::InterpreterNotFound { path } => write!(f, "interpreter '{path}' does not exist"),
            Self::InterpreterNotAFile { path, file_type } => {
                write!(
                    f,
                    "interpreter '{path}' is a {file_type}, not a regular file"
                )
            }
            Self::InterpreterNotExecutable { path } => {
                write!(f, "interpreter '{path}' has no execute permission")
            }
            Self::InterpreterOutsideAllowedRoot {
                resolved,
                allowed_root,
            } => write!(
                f,
                "interpreter resolves to '{resolved}', outside the allowed root '{allowed_root}'"
            ),
            Self::SuppliedDigestMalformed { field, value } => {
                write!(f, "supplied {field} '{value}' is not a sha256 hex digest")
            }
            Self::ExecutableHashMismatch { supplied, probed } => write!(
                f,
                "executable hash mismatch: supplied {supplied}, probed {probed}"
            ),
            Self::InterpreterChangedDuringProbe { before, after } => write!(
                f,
                "interpreter changed during the version probe: before {before}, after {after}"
            ),
            Self::PackageLockHashMismatch { supplied, probed } => write!(
                f,
                "package lock hash mismatch: supplied {supplied}, probed {probed}"
            ),
            Self::PackageLockChangedDuringProbe { before, after } => write!(
                f,
                "package lock changed during the version probe: before {before}, after {after}"
            ),
            Self::PackageLockUnverifiable { supplied } => write!(
                f,
                "package lock hash {supplied} was supplied with no lock file to verify it against"
            ),
            Self::PackageLockNotAFile { path } => {
                write!(f, "package lock '{path}' is not a regular file")
            }
            Self::VersionProbeSpawnFailed { detail } => {
                write!(f, "version probe could not be spawned: {detail}")
            }
            Self::VersionProbeTimedOut { timeout_ms } => {
                write!(f, "version probe exceeded {timeout_ms}ms and was killed")
            }
            Self::VersionProbeExitNonZero {
                exit_code,
                output_excerpt,
            } => write!(
                f,
                "version probe exited with {exit_code:?}: {output_excerpt}"
            ),
            Self::VersionProbeEmptyOutput => {
                write!(f, "version probe produced no output to identify the kernel")
            }
            Self::VersionProbeArgsInvalid { detail } => {
                write!(f, "version probe arguments rejected: {detail}")
            }
        }
    }
}

// ── Policy ────────────────────────────────────────────────────────

/// The sandbox policy an admitted kernel must be executed under.
///
/// These are **requirements placed on the execution seam**, not observations
/// of this machine: nothing in this crate spawns a kernel, so nothing here can
/// claim to have watched a sandbox hold. They are recorded so that a runner
/// which cannot honour them has to fail the step rather than quietly run
/// unsandboxed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelPolicy {
    pub require_no_network: bool,
    pub require_process_isolation: bool,
    pub require_artifact_only_io: bool,
}

impl Default for KernelPolicy {
    fn default() -> Self {
        Self {
            require_no_network: true,
            require_process_isolation: true,
            require_artifact_only_io: true,
        }
    }
}

/// Conservative default caps, applied when the caller states none.
pub fn default_resource_cap() -> ResourceCap {
    ResourceCap {
        max_memory_mb: 2048,
        max_cpu_seconds: 3600,
        max_output_bytes: 100_000_000,
        max_file_descriptors: 64,
    }
}

// ── Request ───────────────────────────────────────────────────────

/// What to probe, and what the caller claims about it.
#[derive(Debug, Clone)]
pub struct KernelAdmissionRequest {
    pub kernel_id: String,
    pub kind: KernelKind,
    /// Must be absolute. This is deliberately not looked up on `PATH`.
    pub interpreter_path: PathBuf,
    /// When set, the interpreter's *resolved* path (symlinks followed) must
    /// live under this root.
    pub allowed_root: Option<PathBuf>,
    /// Caller's claim about the executable digest. Verified against the bytes
    /// on disk; a mismatch rejects. Never copied into the record.
    pub supplied_executable_hash: Option<String>,
    /// Lock file whose digest pins the kernel's package set
    /// (`requirements.lock`, `renv.lock`, `Manifest.toml`, …).
    pub package_lock_path: Option<PathBuf>,
    /// Caller's claim about the lock digest. Requires `package_lock_path`.
    pub supplied_package_lock_hash: Option<String>,
    /// Override for the version argv. Defaults to the per-kind argv.
    pub version_probe_args: Option<Vec<String>>,
    pub probe_timeout: Duration,
    pub policy: KernelPolicy,
    pub resource_cap: ResourceCap,
    /// Identity recorded as the admitting authority. Provenance, not proof.
    pub admitted_by: String,
}

impl KernelAdmissionRequest {
    pub fn new(
        kernel_id: impl Into<String>,
        kind: KernelKind,
        interpreter_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            kernel_id: kernel_id.into(),
            kind,
            interpreter_path: interpreter_path.into(),
            allowed_root: None,
            supplied_executable_hash: None,
            package_lock_path: None,
            supplied_package_lock_hash: None,
            version_probe_args: None,
            probe_timeout: DEFAULT_PROBE_TIMEOUT,
            policy: KernelPolicy::default(),
            resource_cap: default_resource_cap(),
            admitted_by: "unattributed".into(),
        }
    }

    pub fn with_allowed_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.allowed_root = Some(root.into());
        self
    }

    pub fn with_supplied_executable_hash(mut self, hash: impl Into<String>) -> Self {
        self.supplied_executable_hash = Some(hash.into());
        self
    }

    pub fn with_package_lock(mut self, path: impl Into<PathBuf>) -> Self {
        self.package_lock_path = Some(path.into());
        self
    }

    pub fn with_supplied_package_lock_hash(mut self, hash: impl Into<String>) -> Self {
        self.supplied_package_lock_hash = Some(hash.into());
        self
    }

    pub fn with_version_probe_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.version_probe_args = Some(args.into_iter().map(Into::into).collect());
        self
    }

    pub fn with_probe_timeout(mut self, timeout: Duration) -> Self {
        self.probe_timeout = timeout;
        self
    }

    /// Set the sandbox policy the execution seam must enforce.
    ///
    /// Lowering `require_process_isolation` is an operator decision, not a
    /// default, and it is recorded on the resulting admission record so a
    /// reviewer can see under what terms a kernel was admitted.
    pub fn with_policy(mut self, policy: KernelPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn with_admitted_by(mut self, who: impl Into<String>) -> Self {
        self.admitted_by = who.into();
        self
    }

    /// The argv used to ask the interpreter for its version.
    fn version_argv(&self) -> Vec<String> {
        self.version_probe_args.clone().unwrap_or_else(|| {
            match self.kind {
                // `-VV` prints the full build string, not just `3.x.y`.
                KernelKind::Python => vec!["-VV".to_string()],
                KernelKind::R | KernelKind::Julia => vec!["--version".to_string()],
            }
        })
    }
}

// ── Probe ─────────────────────────────────────────────────────────

/// Probe the environment and build an admission record.
///
/// Returns `Ok` with `AdmissionStatus::Rejected` and a
/// [`RejectionReason`] for anything wrong with the *kernel*; `Err` is reserved
/// for a malformed request, which is a caller bug rather than a verdict.
pub fn probe_kernel(request: &KernelAdmissionRequest) -> Result<KernelAdmission> {
    if request.kernel_id.trim().is_empty() {
        return Err(ScienceError::Invalid("kernel_id is required".into()));
    }
    if request.admitted_by.trim().is_empty() {
        return Err(ScienceError::Invalid(
            "admitted_by is required: an admission needs an authority".into(),
        ));
    }
    if request.probe_timeout.is_zero() {
        return Err(ScienceError::Invalid(
            "probe_timeout must be greater than zero".into(),
        ));
    }

    match probe_inner(request) {
        Ok(probed) => Ok(admitted_record(request, probed)),
        Err(reason) => Ok(rejected_record(request, reason)),
    }
}

/// Probe the exact executable capability retained by the SessionActor.
///
/// Unlike [`probe_kernel`], this never reopens `request.interpreter_path`.
/// The path remains evidence and a confinement input; hashing, version
/// execution, and the later workflow runner all use the same non-serializable
/// [`PinnedExecutable`].
pub fn probe_pinned_kernel(
    request: &KernelAdmissionRequest,
    executable: &PinnedExecutable,
) -> Result<KernelAdmission> {
    if request.kernel_id.trim().is_empty() {
        return Err(ScienceError::Invalid("kernel_id is required".into()));
    }
    if request.admitted_by.trim().is_empty() {
        return Err(ScienceError::Invalid(
            "admitted_by is required: an admission needs an authority".into(),
        ));
    }
    if request.probe_timeout.is_zero() {
        return Err(ScienceError::Invalid(
            "probe_timeout must be greater than zero".into(),
        ));
    }

    match probe_pinned_inner(request, executable) {
        Ok(probed) => Ok(admitted_record(request, probed)),
        Err(reason) => Ok(rejected_record(request, reason)),
    }
}

/// Everything the probe learned about a kernel that passed.
struct ProbedKernel {
    resolved_path: PathBuf,
    executable_hash: String,
    package_lock_hash: String,
    exact_version: String,
}

fn probe_pinned_inner(
    request: &KernelAdmissionRequest,
    executable: &PinnedExecutable,
) -> std::result::Result<ProbedKernel, RejectionReason> {
    let resolved = executable.canonical_path();
    let resolved_display = resolved.display().to_string();
    if !request.interpreter_path.is_absolute() {
        return Err(RejectionReason::InterpreterPathNotAbsolute {
            path: request.interpreter_path.display().to_string(),
        });
    }
    if !executable.matches_source_path(&request.interpreter_path) {
        return Err(RejectionReason::InterpreterNotAFile {
            path: request.interpreter_path.display().to_string(),
            file_type: "does not identify the actor-retained executable".into(),
        });
    }
    if let Some(root) = &request.allowed_root {
        let allowed = dunce::canonicalize(root)
            .or_else(|_| std::path::absolute(root))
            .unwrap_or_else(|_| root.clone());
        if !resolved.starts_with(&allowed) {
            return Err(RejectionReason::InterpreterOutsideAllowedRoot {
                resolved: resolved_display.clone(),
                allowed_root: allowed.display().to_string(),
            });
        }
    }

    let executable_hash = executable.sha256().to_string();
    if let Some(supplied) = &request.supplied_executable_hash {
        let supplied =
            normalise_digest(supplied).ok_or_else(|| RejectionReason::SuppliedDigestMalformed {
                field: "executableHash".into(),
                value: supplied.clone(),
            })?;
        if supplied != executable_hash {
            return Err(RejectionReason::ExecutableHashMismatch {
                supplied,
                probed: executable_hash,
            });
        }
    }

    let package_lock_hash = match &request.package_lock_path {
        Some(path) => {
            let lock_display = path.display().to_string();
            let lock_meta =
                fs::symlink_metadata(path).map_err(|_| RejectionReason::PackageLockNotAFile {
                    path: lock_display.clone(),
                })?;
            if !lock_meta.is_file() {
                return Err(RejectionReason::PackageLockNotAFile { path: lock_display });
            }
            hash_file(path)
                .map_err(|_| RejectionReason::PackageLockNotAFile { path: lock_display })?
        }
        None => {
            if let Some(supplied) = &request.supplied_package_lock_hash {
                return Err(RejectionReason::PackageLockUnverifiable {
                    supplied: supplied.clone(),
                });
            }
            NO_PACKAGE_LOCK.to_string()
        }
    };
    if let Some(supplied) = &request.supplied_package_lock_hash {
        let supplied =
            normalise_digest(supplied).ok_or_else(|| RejectionReason::SuppliedDigestMalformed {
                field: "packageLockHash".into(),
                value: supplied.clone(),
            })?;
        if supplied != package_lock_hash {
            return Err(RejectionReason::PackageLockHashMismatch {
                supplied,
                probed: package_lock_hash,
            });
        }
    }

    let exact_version =
        run_pinned_version_probe(executable, &request.version_argv(), request.probe_timeout)?;
    if let Some(lock_path) = &request.package_lock_path {
        let lock_hash_after =
            hash_file(lock_path).map_err(|_| RejectionReason::PackageLockNotAFile {
                path: lock_path.display().to_string(),
            })?;
        if package_lock_hash != lock_hash_after {
            return Err(RejectionReason::PackageLockChangedDuringProbe {
                before: package_lock_hash,
                after: lock_hash_after,
            });
        }
    }

    Ok(ProbedKernel {
        resolved_path: resolved.to_path_buf(),
        executable_hash,
        package_lock_hash,
        exact_version,
    })
}

fn probe_inner(
    request: &KernelAdmissionRequest,
) -> std::result::Result<ProbedKernel, RejectionReason> {
    let given = &request.interpreter_path;
    let given_display = given.display().to_string();

    // 1. Absolute only. A bare name or a relative path would be resolved
    //    through PATH or the cwd, neither of which pins an identity.
    if !given.is_absolute() {
        return Err(RejectionReason::InterpreterPathNotAbsolute {
            path: given_display,
        });
    }

    // 2. Resolve symlinks so the record names the file that will actually run.
    let metadata = fs::symlink_metadata(given).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            RejectionReason::InterpreterNotFound {
                path: given_display.clone(),
            }
        } else {
            RejectionReason::InterpreterNotAFile {
                path: given_display.clone(),
                file_type: format!("unreadable ({error})"),
            }
        }
    })?;
    if metadata.is_dir() {
        return Err(RejectionReason::InterpreterNotAFile {
            path: given_display,
            file_type: "directory".into(),
        });
    }
    let resolved = dunce::canonicalize(given).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            RejectionReason::InterpreterNotFound {
                path: given_display.clone(),
            }
        } else {
            RejectionReason::InterpreterNotAFile {
                path: given_display.clone(),
                file_type: format!("unresolvable ({error})"),
            }
        }
    })?;
    let resolved_display = resolved.display().to_string();

    let resolved_metadata =
        fs::metadata(&resolved).map_err(|error| RejectionReason::InterpreterNotAFile {
            path: resolved_display.clone(),
            file_type: format!("unreadable ({error})"),
        })?;
    if resolved_metadata.is_dir() {
        return Err(RejectionReason::InterpreterNotAFile {
            path: resolved_display,
            file_type: "directory".into(),
        });
    }
    if !resolved_metadata.is_file() {
        return Err(RejectionReason::InterpreterNotAFile {
            path: resolved_display,
            file_type: "non-regular file".into(),
        });
    }

    // 3. Confinement is checked against the *resolved* path, so a symlink
    //    planted inside the allowed root cannot smuggle in an outside binary.
    if let Some(root) = &request.allowed_root {
        let allowed = dunce::canonicalize(root)
            .or_else(|_| std::path::absolute(root))
            .unwrap_or_else(|_| root.clone());
        if !resolved.starts_with(&allowed) {
            return Err(RejectionReason::InterpreterOutsideAllowedRoot {
                resolved: resolved_display,
                allowed_root: allowed.display().to_string(),
            });
        }
    }

    if !is_executable(&resolved, &resolved_metadata) {
        return Err(RejectionReason::InterpreterNotExecutable {
            path: resolved_display,
        });
    }

    // 4. Hash the executable bytes themselves.
    let executable_hash =
        hash_file(&resolved).map_err(|error| RejectionReason::InterpreterNotAFile {
            path: resolved_display.clone(),
            file_type: format!("unreadable ({error})"),
        })?;

    // 5. Verify — never echo — a supplied executable digest.
    if let Some(supplied) = &request.supplied_executable_hash {
        let supplied =
            normalise_digest(supplied).ok_or_else(|| RejectionReason::SuppliedDigestMalformed {
                field: "executableHash".into(),
                value: supplied.clone(),
            })?;
        if supplied != executable_hash {
            return Err(RejectionReason::ExecutableHashMismatch {
                supplied,
                probed: executable_hash,
            });
        }
    }

    // 6. Package lock: hashed when present, and any supplied digest is
    //    verified against it. A claim with nothing to check it against is a
    //    rejection, not a pass.
    let package_lock_hash = match &request.package_lock_path {
        Some(path) => {
            let lock_display = path.display().to_string();
            let lock_meta =
                fs::symlink_metadata(path).map_err(|_| RejectionReason::PackageLockNotAFile {
                    path: lock_display.clone(),
                })?;
            if !lock_meta.is_file() {
                return Err(RejectionReason::PackageLockNotAFile { path: lock_display });
            }
            hash_file(path)
                .map_err(|_| RejectionReason::PackageLockNotAFile { path: lock_display })?
        }
        None => {
            if let Some(supplied) = &request.supplied_package_lock_hash {
                return Err(RejectionReason::PackageLockUnverifiable {
                    supplied: supplied.clone(),
                });
            }
            // Honest sentinel: no lock file was offered, so no package set is
            // pinned. Not a digest, and deliberately not one.
            NO_PACKAGE_LOCK.to_string()
        }
    };
    if let Some(supplied) = &request.supplied_package_lock_hash {
        let supplied =
            normalise_digest(supplied).ok_or_else(|| RejectionReason::SuppliedDigestMalformed {
                field: "packageLockHash".into(),
                value: supplied.clone(),
            })?;
        if supplied != package_lock_hash {
            return Err(RejectionReason::PackageLockHashMismatch {
                supplied,
                probed: package_lock_hash,
            });
        }
    }

    // 7. The version comes from running the interpreter, under a timeout.
    let argv = request.version_argv();
    let exact_version = run_version_probe(&resolved, &argv, request.probe_timeout)?;

    // 8. Close the time-of-check/time-of-use window. A probe may execute code
    //    that replaces its own executable or rewrites the dependency lock.
    //    Re-hash both after execution and only commit a record when the bytes
    //    observed before and after are identical.
    let executable_hash_after =
        hash_file(&resolved).map_err(|error| RejectionReason::InterpreterNotAFile {
            path: resolved_display,
            file_type: format!("unreadable after probe ({error})"),
        })?;
    if executable_hash != executable_hash_after {
        return Err(RejectionReason::InterpreterChangedDuringProbe {
            before: executable_hash,
            after: executable_hash_after,
        });
    }
    if let Some(lock_path) = &request.package_lock_path {
        let lock_hash_after =
            hash_file(lock_path).map_err(|_| RejectionReason::PackageLockNotAFile {
                path: lock_path.display().to_string(),
            })?;
        if package_lock_hash != lock_hash_after {
            return Err(RejectionReason::PackageLockChangedDuringProbe {
                before: package_lock_hash,
                after: lock_hash_after,
            });
        }
    }

    Ok(ProbedKernel {
        resolved_path: resolved,
        executable_hash: executable_hash_after,
        package_lock_hash,
        exact_version,
    })
}

/// Recorded in `package_lock_hash` when the caller pinned no lock file.
pub const NO_PACKAGE_LOCK: &str = "none:no-package-lock-supplied";

fn admitted_record(request: &KernelAdmissionRequest, probed: ProbedKernel) -> KernelAdmission {
    KernelAdmission {
        kernel_id: request.kernel_id.clone(),
        kind: request.kind,
        exact_version: probed.exact_version,
        executable_hash: probed.executable_hash,
        package_lock_hash: probed.package_lock_hash,
        interpreter_path: probed.resolved_path.display().to_string(),
        os: std::env::consts::OS.to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        default_no_network: request.policy.require_no_network,
        process_isolation: request.policy.require_process_isolation,
        resource_cap: request.resource_cap,
        artifact_only_io: request.policy.require_artifact_only_io,
        admission_status: AdmissionStatus::Admitted,
        rejection_reason: None,
        admitted_at: Some(chrono::Utc::now().to_rfc3339()),
        admitted_by: Some(request.admitted_by.clone()),
    }
}

fn rejected_record(request: &KernelAdmissionRequest, reason: RejectionReason) -> KernelAdmission {
    KernelAdmission {
        kernel_id: request.kernel_id.clone(),
        kind: request.kind,
        // Nothing was learned, and the record says so rather than guessing.
        exact_version: String::new(),
        executable_hash: String::new(),
        package_lock_hash: String::new(),
        interpreter_path: request.interpreter_path.display().to_string(),
        os: std::env::consts::OS.to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        default_no_network: request.policy.require_no_network,
        process_isolation: request.policy.require_process_isolation,
        resource_cap: request.resource_cap,
        artifact_only_io: request.policy.require_artifact_only_io,
        admission_status: AdmissionStatus::Rejected,
        rejection_reason: Some(reason),
        // A rejected kernel was never admitted, so it carries no admitter.
        admitted_at: None,
        admitted_by: None,
    }
}

fn is_executable(path: &Path, metadata: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = path;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        // Windows has no execute bit; the loader decides by extension.
        let _ = metadata;
        matches!(
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("exe") | Some("bat") | Some("cmd") | Some("com")
        )
    }
}

fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
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

/// Accept `sha256:<hex>` or bare `<hex>`; anything else is not a digest.
fn normalise_digest(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let raw = trimmed
        .strip_prefix("sha256:")
        .or_else(|| trimmed.strip_prefix("SHA256:"))
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    (raw.len() == 64 && raw.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(raw)
}

/// Execute the interpreter to read its version, killing it at the deadline.
///
/// Output goes to bounded, parent-owned temporary files rather than pipes.
/// A forked descendant can inherit a pipe after the direct child exits and
/// make a reader-thread join hang forever; a regular file has no EOF wait.
/// On Unix the probe starts in its own process group. Every exit path kills
/// that group before returning and reaps the direct child; orphaned
/// descendants may briefly remain as non-running zombies until the OS reaps
/// them, but cannot keep the capture open or continue executing.
fn run_version_probe(
    executable: &Path,
    argv: &[String],
    timeout: Duration,
) -> std::result::Result<String, RejectionReason> {
    validate_version_argv(argv)?;
    let mut stdout = ProbeCapture::new("stdout").map_err(probe_io_error)?;
    let mut stderr = ProbeCapture::new("stderr").map_err(probe_io_error)?;
    let stdout_stdio = stdout.stdio().map_err(probe_io_error)?;
    let stderr_stdio = stderr.stdio().map_err(probe_io_error)?;
    let mut command = Command::new(executable);
    command
        .args(argv)
        .stdin(Stdio::null())
        .stdout(stdout_stdio)
        .stderr(stderr_stdio)
        .env("LC_ALL", "C")
        .env("LANG", "C");
    configure_probe_process(&mut command);
    let child = command
        .spawn()
        .map_err(|error| RejectionReason::VersionProbeSpawnFailed {
            detail: error.to_string(),
        })?;
    complete_version_probe(child, &mut stdout, &mut stderr, timeout)
}

fn run_pinned_version_probe(
    executable: &PinnedExecutable,
    argv: &[String],
    timeout: Duration,
) -> std::result::Result<String, RejectionReason> {
    validate_version_argv(argv)?;
    let mut stdout = ProbeCapture::new("stdout").map_err(probe_io_error)?;
    let mut stderr = ProbeCapture::new("stderr").map_err(probe_io_error)?;
    let stdout_stdio = stdout.stdio().map_err(probe_io_error)?;
    let stderr_stdio = stderr.stdio().map_err(probe_io_error)?;
    let mut pinned =
        executable
            .spawn_command()
            .map_err(|error| RejectionReason::VersionProbeSpawnFailed {
                detail: error.to_string(),
            })?;
    pinned
        .command_mut()
        .args(argv)
        .stdin(Stdio::null())
        .stdout(stdout_stdio)
        .stderr(stderr_stdio)
        .env("LC_ALL", "C")
        .env("LANG", "C");
    configure_probe_process(pinned.command_mut());
    let child = pinned
        .spawn()
        .map_err(|error| RejectionReason::VersionProbeSpawnFailed {
            detail: error.to_string(),
        })?;
    // `pinned` remains alive until the child has been spawned. On Linux the
    // child inherited its memfd; on macOS exec opened the private snapshot.
    complete_version_probe(child, &mut stdout, &mut stderr, timeout)
}

fn validate_version_argv(argv: &[String]) -> std::result::Result<(), RejectionReason> {
    for arg in argv {
        if arg.contains('\0') || arg.contains('\n') || arg.contains('\r') {
            return Err(RejectionReason::VersionProbeArgsInvalid {
                detail: "arguments may not contain NUL or line breaks".into(),
            });
        }
    }
    Ok(())
}

fn complete_version_probe(
    mut child: Child,
    stdout: &mut ProbeCapture,
    stderr: &mut ProbeCapture,
    timeout: Duration,
) -> std::result::Result<String, RejectionReason> {
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                terminate_probe_processes(&mut child).map_err(probe_io_error)?;
                break status;
            }
            Ok(None) => {}
            Err(error) => {
                let termination_error = terminate_probe_processes(&mut child).err();
                return Err(RejectionReason::VersionProbeSpawnFailed {
                    detail: match termination_error {
                        Some(termination_error) => format!(
                            "{error}; process-group termination also failed: {termination_error}"
                        ),
                        None => error.to_string(),
                    },
                });
            }
        }
        if started.elapsed() >= timeout {
            terminate_probe_processes(&mut child).map_err(probe_io_error)?;
            return Err(RejectionReason::VersionProbeTimedOut {
                timeout_ms: timeout.as_millis() as u64,
            });
        }
        thread::sleep(Duration::from_millis(5));
    };

    let out = stdout.read_capped().map_err(probe_io_error)?;
    let err = stderr.read_capped().map_err(probe_io_error)?;

    // Python <=3.3 wrote `-V` to stderr, R and Julia write to stdout: read both.
    let combined = format!("{out}\n{err}");
    if !status.success() {
        return Err(RejectionReason::VersionProbeExitNonZero {
            exit_code: status.code(),
            output_excerpt: excerpt(&combined),
        });
    }

    let version = combined
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .chars()
        .take(MAX_VERSION_CHARS)
        .collect::<String>();
    if version.is_empty() {
        return Err(RejectionReason::VersionProbeEmptyOutput);
    }
    Ok(version)
}

fn probe_io_error(error: std::io::Error) -> RejectionReason {
    RejectionReason::VersionProbeSpawnFailed {
        detail: error.to_string(),
    }
}

struct ProbeCapture {
    path: PathBuf,
    file: Option<fs::File>,
}

impl ProbeCapture {
    fn new(stream: &str) -> std::io::Result<Self> {
        use std::fs::OpenOptions;
        #[cfg(unix)]
        use std::os::unix::fs::OpenOptionsExt;

        for _ in 0..16 {
            let path = std::env::temp_dir().join(format!(
                "lumen-science-kernel-probe-{}-{stream}-{}",
                std::process::id(),
                Uuid::now_v7()
            ));
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            match options.open(&path) {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        file: Some(file),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not allocate a unique kernel probe capture",
        ))
    }

    fn stdio(&self) -> std::io::Result<Stdio> {
        Ok(Stdio::from(
            self.file
                .as_ref()
                .ok_or_else(|| std::io::Error::other("probe capture is closed"))?
                .try_clone()?,
        ))
    }

    fn read_capped(&mut self) -> std::io::Result<String> {
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| std::io::Error::other("probe capture is closed"))?;
        file.seek(SeekFrom::Start(0))?;
        let mut buffer = Vec::new();
        file.take(MAX_PROBE_OUTPUT_BYTES as u64)
            .read_to_end(&mut buffer)?;
        Ok(String::from_utf8_lossy(&buffer).into_owned())
    }
}

impl Drop for ProbeCapture {
    fn drop(&mut self) {
        self.file.take();
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(unix)]
fn configure_probe_process(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    // SAFETY: the closure performs only async-signal-safe libc calls before
    // exec. It does not allocate or acquire locks.
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            let mut limit = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if libc::getrlimit(libc::RLIMIT_FSIZE, &mut limit) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            let bounded_output = limit.rlim_max.min(MAX_PROBE_OUTPUT_BYTES as libc::rlim_t);
            limit.rlim_cur = bounded_output;
            limit.rlim_max = bounded_output;
            if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn configure_probe_process(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_probe_processes(child: &mut Child) -> std::io::Result<()> {
    let process_group = -(child.id() as libc::pid_t);
    // SAFETY: `process_group` is the negative PID assigned by the setpgid
    // pre-exec hook, so this targets only this probe's process group.
    let killed = unsafe { libc::kill(process_group, libc::SIGKILL) };
    let kill_error = (killed == -1).then(std::io::Error::last_os_error);
    let _ = child.kill();
    let wait_result = child.wait();
    if let Some(error) = kill_error
        && error.raw_os_error() != Some(libc::ESRCH)
    {
        return Err(error);
    }
    wait_result.map(|_| ())
}

#[cfg(not(unix))]
fn terminate_probe_processes(child: &mut Child) -> std::io::Result<()> {
    // A regular-file capture still prevents the inherited-pipe hang on
    // Windows. std does not provide a Job Object API, so this can only reap
    // the direct child; product acceptance must not claim Windows process-tree
    // confinement until a Job Object implementation is added.
    let _ = child.kill();
    child.wait().map(|_| ())
}

fn excerpt(text: &str) -> String {
    text.trim().chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn request(path: impl Into<PathBuf>) -> KernelAdmissionRequest {
        KernelAdmissionRequest::new("py-test", KernelKind::Python, path)
            .with_admitted_by("test-suite")
            // Generous on purpose. These tests assert WHICH rejection reason a
            // probe produces, not how fast it runs, so the budget only needs to
            // exceed scheduling delay. Five seconds was enough on an idle
            // machine and not enough on a loaded one — every probe test failed
            // with `version_probe_timed_out` while two other builds were
            // running. A shared CI runner is a loaded machine, so a budget
            // tuned for an idle one is a flake waiting to happen, and a flaky
            // test is worse than none: it teaches people to ignore failures.
            // The single test that does assert timing sets its own 300ms.
            .with_probe_timeout(Duration::from_secs(60))
    }

    /// Write an executable shebang script. Unix-only: there is no portable
    /// way to make a text file executable on Windows.
    #[cfg(unix)]
    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    fn process_is_still_running(pid: libc::pid_t) -> bool {
        // `kill(pid, 0)` reports true for zombies, so it cannot by itself
        // prove that a SIGKILLed descendant is still executing.
        // SAFETY: signal 0 performs no mutation and probes the child-reported
        // PID only.
        if unsafe { libc::kill(pid, 0) } == -1
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
        {
            return false;
        }
        let output = Command::new("ps")
            .args(["-o", "state=", "-p", &pid.to_string()])
            .output()
            .expect("ps must be available for the Unix process-state assertion");
        if !output.status.success() {
            // The process disappeared between kill(0) and ps.
            return false;
        }
        let state = String::from_utf8_lossy(&output.stdout);
        let state = state.trim();
        !state.is_empty() && !state.starts_with('Z')
    }

    fn reason(admission: &KernelAdmission) -> &RejectionReason {
        assert_eq!(
            admission.admission_status,
            AdmissionStatus::Rejected,
            "expected a rejection, got {admission:?}"
        );
        admission
            .rejection_reason
            .as_ref()
            .expect("a rejection must carry a reason")
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn pinned_probe_uses_retained_bytes_after_the_original_path_is_replaced() {
        let dir = tempdir().unwrap();
        let path = script(dir.path(), "python", "#!/bin/sh\nprintf 'Pinned 1.0\\n'\n");
        let original = fs::read(&path).unwrap();
        let pinned = PinnedExecutable::pin(&path).expect("pin executable");

        fs::rename(&path, dir.path().join("approved-original")).unwrap();
        script(
            dir.path(),
            "python",
            "#!/bin/sh\nprintf 'Replacement 9.9\\n'\n",
        );

        let admission = probe_pinned_kernel(
            &request(&path).with_version_probe_args(std::iter::empty::<&str>()),
            &pinned,
        )
        .expect("probe pinned executable");
        assert_eq!(admission.admission_status, AdmissionStatus::Admitted);
        assert_eq!(admission.exact_version, "Pinned 1.0");
        assert_eq!(
            admission.executable_hash,
            format!("{:x}", Sha256::digest(&original))
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn pinned_probe_does_not_reopen_a_missing_original_path() {
        let dir = tempdir().unwrap();
        let path = script(dir.path(), "python", "#!/bin/sh\nprintf 'Pinned 1.0\\n'\n");
        let pinned = PinnedExecutable::pin(&path).expect("pin executable");
        fs::rename(&path, dir.path().join("approved-original")).unwrap();
        assert!(!path.exists(), "the approved pathname must be absent");

        let admission = probe_pinned_kernel(
            &request(&path).with_version_probe_args(std::iter::empty::<&str>()),
            &pinned,
        )
        .expect("probe retained executable without reopening its old path");
        assert_eq!(admission.admission_status, AdmissionStatus::Admitted);
        assert_eq!(admission.exact_version, "Pinned 1.0");
    }

    // ── Negative cases ────────────────────────────────────────────

    #[test]
    fn relative_interpreter_path_is_rejected() {
        // The old code accepted this without looking: `python3` names whatever
        // the ambient PATH happens to resolve, which is not an identity.
        let admission = probe_kernel(&request("python3")).unwrap();
        assert_eq!(reason(&admission).code(), "interpreter_path_not_absolute");
        assert!(!admission.is_safe());
        assert!(admission.admitted_at.is_none());
        assert!(admission.admitted_by.is_none());
    }

    #[test]
    fn missing_interpreter_is_rejected() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("no-such-python");
        let admission = probe_kernel(&request(missing)).unwrap();
        assert_eq!(reason(&admission).code(), "interpreter_not_found");
        assert!(admission.exact_version.is_empty());
        assert!(admission.executable_hash.is_empty());
    }

    #[test]
    fn directory_interpreter_is_rejected() {
        let dir = tempdir().unwrap();
        let subdir = dir.path().join("bin");
        fs::create_dir(&subdir).unwrap();
        let admission = probe_kernel(&request(subdir)).unwrap();
        assert_eq!(reason(&admission).code(), "interpreter_not_a_file");
    }

    #[cfg(unix)]
    #[test]
    fn non_executable_file_is_rejected() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("python3");
        fs::write(&path, "#!/bin/sh\necho 3.12.0\n").unwrap();
        // Deliberately no execute bit.
        let admission = probe_kernel(&request(path)).unwrap();
        assert_eq!(reason(&admission).code(), "interpreter_not_executable");
    }

    #[cfg(unix)]
    #[test]
    fn supplied_executable_hash_mismatch_is_rejected() {
        let dir = tempdir().unwrap();
        let path = script(dir.path(), "python3", "#!/bin/sh\necho 'Python 3.12.0'\n");
        let admission =
            probe_kernel(&request(&path).with_supplied_executable_hash("b".repeat(64))).unwrap();
        let reason = reason(&admission);
        assert_eq!(reason.code(), "executable_hash_mismatch");
        // The supplied digest must not survive into the record.
        assert!(admission.executable_hash.is_empty());
        assert!(
            !format!("{reason}").contains("probed b"),
            "probed digest must be the real one"
        );
    }

    #[cfg(unix)]
    #[test]
    fn supplied_executable_hash_that_matches_is_admitted() {
        let dir = tempdir().unwrap();
        let path = script(dir.path(), "python3", "#!/bin/sh\necho 'Python 3.12.0'\n");
        let real = hash_file(&path).unwrap();
        let admission =
            probe_kernel(&request(&path).with_supplied_executable_hash(format!("sha256:{real}")))
                .unwrap();
        assert_eq!(admission.admission_status, AdmissionStatus::Admitted);
        assert_eq!(admission.executable_hash, real);
    }

    #[test]
    fn malformed_supplied_digest_is_rejected() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("python3");
        fs::write(&path, "x").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let admission = probe_kernel(&request(&path).with_supplied_executable_hash("abc")).unwrap();
        // On non-unix the executable check fires first; either way it is a
        // rejection and never an admission.
        assert_eq!(admission.admission_status, AdmissionStatus::Rejected);
        #[cfg(unix)]
        assert_eq!(reason(&admission).code(), "supplied_digest_malformed");
    }

    #[cfg(unix)]
    #[test]
    fn version_probe_timeout_is_rejected() {
        let dir = tempdir().unwrap();
        let path = script(dir.path(), "slow", "#!/bin/sh\nsleep 30\n");
        let started = Instant::now();
        let admission =
            probe_kernel(&request(&path).with_probe_timeout(Duration::from_millis(300))).unwrap();
        assert_eq!(reason(&admission).code(), "version_probe_timed_out");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "forked probe descendants held the timeout path open"
        );
    }

    #[cfg(unix)]
    #[test]
    fn inherited_probe_output_does_not_block_after_direct_child_exits() {
        let dir = tempdir().unwrap();
        let descendant_pid = dir.path().join("descendant.pid");
        let path = script(
            dir.path(),
            "forking",
            &format!(
                "#!/bin/sh\nsleep 30 &\nprintf '%s' \"$!\" > '{}'\necho 'Python 3.12.0'\n",
                descendant_pid.display()
            ),
        );
        let started = Instant::now();
        let admission =
            probe_kernel(&request(&path).with_probe_timeout(Duration::from_secs(2))).unwrap();
        assert_eq!(
            admission.admission_status,
            AdmissionStatus::Admitted,
            "{admission:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "inherited output handle delayed probe completion"
        );
        let pid: libc::pid_t = fs::read_to_string(descendant_pid).unwrap().parse().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while process_is_still_running(pid) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !process_is_still_running(pid),
            "forked descendant remained runnable after process-group termination"
        );
    }

    #[cfg(unix)]
    #[test]
    fn version_probe_non_zero_exit_is_rejected() {
        let dir = tempdir().unwrap();
        let path = script(dir.path(), "broken", "#!/bin/sh\necho boom >&2\nexit 3\n");
        let admission = probe_kernel(&request(&path)).unwrap();
        let reason = reason(&admission);
        assert_eq!(reason.code(), "version_probe_exit_non_zero");
        assert!(format!("{reason}").contains("boom"), "{reason}");
    }

    #[cfg(unix)]
    #[test]
    fn version_probe_with_no_output_is_rejected() {
        let dir = tempdir().unwrap();
        let path = script(dir.path(), "silent", "#!/bin/sh\nexit 0\n");
        let admission = probe_kernel(&request(&path)).unwrap();
        assert_eq!(reason(&admission).code(), "version_probe_empty_output");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_pointing_outside_the_allowed_root_is_rejected() {
        use std::os::unix::fs::symlink;
        let outside = tempdir().unwrap();
        let inside = tempdir().unwrap();
        let real = script(
            outside.path(),
            "python3",
            "#!/bin/sh\necho 'Python 3.12.0'\n",
        );
        let link = inside.path().join("python3");
        symlink(&real, &link).unwrap();

        let admission = probe_kernel(&request(&link).with_allowed_root(inside.path())).unwrap();
        assert_eq!(
            reason(&admission).code(),
            "interpreter_outside_allowed_root"
        );

        // The same interpreter inside the root is admitted, so the rejection
        // is about confinement and not about symlinks in general.
        let confined = script(
            inside.path(),
            "python3-real",
            "#!/bin/sh\necho 'Python 3.12.0'\n",
        );
        let confined_link = inside.path().join("python3-link");
        symlink(&confined, &confined_link).unwrap();
        let ok = probe_kernel(
            &KernelAdmissionRequest::new("py", KernelKind::Python, &confined_link)
                .with_admitted_by("test-suite")
                .with_allowed_root(inside.path()),
        )
        .unwrap();
        assert_eq!(ok.admission_status, AdmissionStatus::Admitted);
    }

    #[cfg(unix)]
    #[test]
    fn package_lock_hash_without_a_lock_file_is_rejected() {
        let dir = tempdir().unwrap();
        let path = script(dir.path(), "python3", "#!/bin/sh\necho 'Python 3.12.0'\n");
        let admission =
            probe_kernel(&request(&path).with_supplied_package_lock_hash("c".repeat(64))).unwrap();
        assert_eq!(reason(&admission).code(), "package_lock_unverifiable");
    }

    #[cfg(unix)]
    #[test]
    fn package_lock_hash_mismatch_is_rejected() {
        let dir = tempdir().unwrap();
        let path = script(dir.path(), "python3", "#!/bin/sh\necho 'Python 3.12.0'\n");
        let lock = dir.path().join("requirements.lock");
        fs::write(&lock, "numpy==2.0.0\n").unwrap();
        let admission = probe_kernel(
            &request(&path)
                .with_package_lock(&lock)
                .with_supplied_package_lock_hash("d".repeat(64)),
        )
        .unwrap();
        assert_eq!(reason(&admission).code(), "package_lock_hash_mismatch");
    }

    #[cfg(unix)]
    #[test]
    fn package_lock_hash_that_matches_is_admitted_and_probed() {
        let dir = tempdir().unwrap();
        let path = script(dir.path(), "python3", "#!/bin/sh\necho 'Python 3.12.0'\n");
        let lock = dir.path().join("requirements.lock");
        fs::write(&lock, "numpy==2.0.0\n").unwrap();
        let real = hash_file(&lock).unwrap();
        let admission = probe_kernel(
            &request(&path)
                .with_package_lock(&lock)
                .with_supplied_package_lock_hash(&real),
        )
        .unwrap();
        assert_eq!(admission.admission_status, AdmissionStatus::Admitted);
        assert_eq!(admission.package_lock_hash, real);
    }

    #[cfg(unix)]
    #[test]
    fn interpreter_that_changes_itself_during_probe_is_rejected() {
        let dir = tempdir().unwrap();
        let path = script(
            dir.path(),
            "python3",
            "#!/bin/sh\nprintf '#!/bin/sh\\necho replaced\\n' > \"$0.replacement\"\nchmod +x \"$0.replacement\"\nmv \"$0.replacement\" \"$0\"\necho 'Python 3.12.0'\n",
        );
        let admission = probe_kernel(&request(&path)).unwrap();
        assert_eq!(
            reason(&admission).code(),
            "interpreter_changed_during_probe"
        );
        assert!(admission.executable_hash.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn package_lock_that_changes_during_probe_is_rejected() {
        let dir = tempdir().unwrap();
        let lock = dir.path().join("requirements.lock");
        fs::write(&lock, "numpy==2.0.0\n").unwrap();
        let path = script(
            dir.path(),
            "python3",
            &format!(
                "#!/bin/sh\nprintf 'numpy==2.1.0\\n' > '{}'\necho 'Python 3.12.0'\n",
                lock.display()
            ),
        );
        let admission = probe_kernel(&request(&path).with_package_lock(&lock)).unwrap();
        assert_eq!(
            reason(&admission).code(),
            "package_lock_changed_during_probe"
        );
        assert!(admission.package_lock_hash.is_empty());
    }

    #[test]
    fn probe_rejects_a_malformed_request_rather_than_admitting() {
        let dir = tempdir().unwrap();
        let mut bad = request(dir.path().join("x"));
        bad.kernel_id = "  ".into();
        assert!(matches!(probe_kernel(&bad), Err(ScienceError::Invalid(_))));

        let mut no_authority = request(dir.path().join("x"));
        no_authority.admitted_by = String::new();
        assert!(matches!(
            probe_kernel(&no_authority),
            Err(ScienceError::Invalid(_))
        ));

        let mut zero_timeout = request(dir.path().join("x"));
        zero_timeout.probe_timeout = Duration::ZERO;
        assert!(matches!(
            probe_kernel(&zero_timeout),
            Err(ScienceError::Invalid(_))
        ));
    }

    // ── Positive case ─────────────────────────────────────────────

    #[cfg(unix)]
    #[test]
    fn a_real_interpreter_is_probed_not_fabricated() {
        let dir = tempdir().unwrap();
        let path = script(
            dir.path(),
            "python3",
            "#!/bin/sh\necho 'Python 3.12.4 (main, Jun  6 2026) [Clang 17.0.0]'\n",
        );
        let admission = probe_kernel(&request(&path).with_admitted_by("ls5-k1")).unwrap();

        assert_eq!(admission.admission_status, AdmissionStatus::Admitted);
        assert!(admission.is_safe());
        // Version came from executing the interpreter, not from "unknown".
        assert!(
            admission.exact_version.starts_with("Python 3.12.4"),
            "version was {:?}",
            admission.exact_version
        );
        // Hash is of the file, verifiable independently.
        assert_eq!(admission.executable_hash, hash_file(&path).unwrap());
        assert_eq!(admission.executable_hash.len(), 64);
        assert_eq!(admission.package_lock_hash, NO_PACKAGE_LOCK);
        assert_eq!(admission.os, std::env::consts::OS);
        assert_eq!(admission.architecture, std::env::consts::ARCH);
        assert_eq!(
            admission.interpreter_path,
            dunce::canonicalize(&path).unwrap().display().to_string()
        );
        assert_eq!(admission.admitted_by.as_deref(), Some("ls5-k1"));
        // A real RFC3339 timestamp, not the hardcoded "2026-07-26".
        let admitted_at = admission.admitted_at.clone().unwrap();
        chrono::DateTime::parse_from_rfc3339(&admitted_at)
            .unwrap_or_else(|error| panic!("admitted_at {admitted_at:?} not rfc3339: {error}"));
    }

    #[cfg(unix)]
    #[test]
    fn two_different_interpreters_get_different_hashes() {
        let dir = tempdir().unwrap();
        let a = script(dir.path(), "a", "#!/bin/sh\necho 'Python 3.11.0'\n");
        let b = script(dir.path(), "b", "#!/bin/sh\necho 'Python 3.12.0'\n");
        let ka = probe_kernel(&request(&a)).unwrap();
        let kb = probe_kernel(&request(&b)).unwrap();
        assert_ne!(ka.executable_hash, kb.executable_hash);
        assert_ne!(ka.exact_version, kb.exact_version);
    }

    #[cfg(unix)]
    #[test]
    fn version_argv_is_argv_never_a_shell_line() {
        let dir = tempdir().unwrap();
        // The script echoes its own arguments; if the executor went through a
        // shell, the `;` would split into a second command.
        let path = script(dir.path(), "echoargs", "#!/bin/sh\necho \"got:$1\"\n");
        let admission =
            probe_kernel(&request(&path).with_version_probe_args(["x; echo INJECTED"])).unwrap();
        assert_eq!(admission.admission_status, AdmissionStatus::Admitted);
        assert_eq!(admission.exact_version, "got:x; echo INJECTED");
        assert!(!admission.exact_version.contains("INJECTED\n"));
    }

    #[test]
    fn probe_args_with_control_characters_are_refused() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("python3");
        fs::write(&path, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let admission =
            probe_kernel(&request(&path).with_version_probe_args(["--v\nersion"])).unwrap();
        assert_eq!(admission.admission_status, AdmissionStatus::Rejected);
        #[cfg(unix)]
        assert_eq!(reason(&admission).code(), "version_probe_args_invalid");
    }

    #[test]
    fn digest_normalisation_accepts_prefixed_and_bare_hex_only() {
        let hex = "a".repeat(64);
        assert_eq!(normalise_digest(&hex).as_deref(), Some(hex.as_str()));
        assert_eq!(
            normalise_digest(&format!("sha256:{hex}")).as_deref(),
            Some(hex.as_str())
        );
        assert_eq!(
            normalise_digest(&"A".repeat(64)).as_deref(),
            Some(hex.as_str())
        );
        assert!(normalise_digest("abc").is_none());
        assert!(normalise_digest(&"z".repeat(64)).is_none());
        assert!(normalise_digest("").is_none());
    }
}
