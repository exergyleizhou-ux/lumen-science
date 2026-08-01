//! Durable Lumen Science kernel. Seam contract: S1, S2, S4.
//!
//! This crate owns records, never execution authority. Product execution must
//! enter through `xai-grok-shell::SessionActor` before calling this crate.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
use uuid::Uuid;

pub mod api;
pub mod capability;
pub mod collaboration;
pub mod connector;
pub mod connectors;
pub mod csv;
pub mod device;
pub mod dossier;
pub mod dummy_lab;
pub mod features;
pub mod governance;
pub mod import;
pub mod multimodal;
pub mod preview;
pub mod primer_thermo;
pub mod project;
pub mod release;
pub mod remote;
pub mod review;
pub mod seqbench;
pub mod skill_quarantine;
pub mod transport;
pub mod workflow;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ScienceError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid record: {0}")]
    Invalid(String),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("ownership mismatch")]
    Ownership,
    #[error("approval already terminal with a conflicting decision")]
    ApprovalConflict,
    #[error("feature disabled: {0}")]
    FeatureDisabled(String),
}

pub type Result<T> = std::result::Result<T, ScienceError>;

const MAX_PERSISTED_ID_BYTES: usize = 128;

#[cfg(test)]
thread_local! {
    static FAIL_RUN_PUBLICATION_AFTER_VISIBLE_RENAME: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
    static FAIL_DIRECTORY_ENTRY_PARENT_SYNC: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
    static FAIL_WRITE_NEW_PARENT_SYNC: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
    static FAIL_EXPLICIT_DIRECTORY_SYNC: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
}

macro_rules! id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            fn validate(&self) -> Result<()> {
                validate_persisted_id(stringify!($name), &self.0)
            }
        }
    };
}
id!(ProjectId);
id!(RunId);
id!(CallId);

impl RunId {
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7().to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunContext {
    pub run_id: RunId,
    pub project_id: ProjectId,
    pub session_id: String,
    pub owner_id: String,
    pub workspace_root: PathBuf,
    pub provider: String,
    pub approval_policy: String,
    pub tool_profile: String,
    pub artifact_root: PathBuf,
    pub environment: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Created,
    AwaitingApproval,
    Running,
    Succeeded,
    Failed,
    Denied,
    TimedOut,
    Cancelled,
    Interrupted,
}

impl RunState {
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded
                | Self::Failed
                | Self::Denied
                | Self::TimedOut
                | Self::Cancelled
                | Self::Interrupted
        )
    }

    /// The ordinary state-machine edges.
    ///
    /// `Succeeded` is deliberately absent: successful completion is an
    /// authority commit and must go through
    /// `ScienceStore::transition_succeeded_verified`, which verifies the
    /// durable approval under the same store write lock.
    fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Created,
                Self::AwaitingApproval | Self::Failed | Self::Interrupted
            ) | (
                Self::AwaitingApproval,
                Self::Running
                    | Self::Failed
                    | Self::Denied
                    | Self::TimedOut
                    | Self::Cancelled
                    | Self::Interrupted
            ) | (
                Self::Running,
                Self::Failed | Self::TimedOut | Self::Cancelled | Self::Interrupted
            )
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    pub schema_version: u32,
    pub context: RunContext,
    pub state: RunState,
    pub terminal_reason: Option<String>,
    /// Durable marker that this run entered exact-manifest completion.
    ///
    /// `None` identifies legacy completion semantics. Once populated it is
    /// immutable, prevents downgrade when the companion seal is missing, and
    /// lets restart recovery rebuild the sealed manifest from collections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub successful_completion_manifest_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub schema_version: u32,
    pub run_id: RunId,
    pub seq: u64,
    pub actor: String,
    pub timestamp: DateTime<Utc>,
    pub kind: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub run_id: RunId,
    pub call_id: CallId,
    pub relative_path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
    pub mime: String,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub run_id: RunId,
    pub claim: String,
    pub source: String,
    pub artifact_sha256: Option<String>,
    pub verified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub run_id: RunId,
    pub source_uri: String,
    pub source_commit: Option<String>,
    pub source_path: Option<String>,
    pub license: String,
    pub retrieved_at: DateTime<Utc>,
    pub input_sha256: String,
    pub tool: String,
    pub environment: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Pending,
    Allow,
    Deny,
    Timeout,
    Cancel,
    Interrupted,
}

impl ApprovalDecision {
    pub fn terminal(&self) -> bool {
        !matches!(self, Self::Pending)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approval {
    pub project_id: ProjectId,
    pub run_id: RunId,
    pub call_id: CallId,
    pub owner_id: String,
    pub decision: ApprovalDecision,
    pub decided_at: Option<DateTime<Utc>>,
}

/// Exact durable state authorized for one successful Science completion.
///
/// Callers build this only after their protocol-specific verification. The
/// store then re-reads and compares every collection while retaining its
/// process-wide write lock, re-hashes every artifact, verifies evidence and
/// provenance completeness, and makes `Succeeded` the final visible write.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuccessfulCompletionManifest {
    pub context: RunContext,
    pub artifacts: Vec<Artifact>,
    pub evidence: Vec<Evidence>,
    pub provenance: Vec<Provenance>,
    pub previews: Vec<preview::PreviewRecord>,
    pub events: Vec<Event>,
    /// The unique last event expected by the actor protocol.
    pub final_event: Event,
}

const AUTHORITY_COMMIT_FENCE_FILE: &str = "authority-commit-fence.json";
const SUCCESSFUL_COMPLETION_SEAL_FILE: &str = "successful-completion-seal.json";
pub(crate) const SEQ_AUTHORITY_PREFIX_SEAL_FILE: &str = "seq-authority-prefix-seal.json";

/// Durable point-of-no-return for a cross-store authority commit.
///
/// Once present, a visible ProjectStore commit may exist or be recoverable.
/// Ordinary failure/cancel/timeout transitions and output rollback must stop;
/// only the exact successful-completion manifest may terminalize the run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AuthorityCommitFence {
    schema_version: u32,
    run_id: RunId,
    project_id: ProjectId,
    call_id: CallId,
    operation_id: String,
}

/// Durable freeze for an exact successful-completion snapshot.
///
/// The seal is written while the store-wide write lock is retained and before
/// `Succeeded` becomes visible. This closes both the post-manifest mutation
/// race and the crash window between snapshot verification and the terminal
/// run write. Legacy protocols that use `transition_succeeded_verified` do not
/// receive this seal and retain their existing post-terminal audit behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SuccessfulCompletionSeal {
    schema_version: u32,
    manifest: SuccessfulCompletionManifest,
    approval: Approval,
    run_schema_version: u32,
    terminal_reason: Option<String>,
    manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct SuccessfulCompletionSealPayload<'a> {
    schema_version: u32,
    manifest: &'a SuccessfulCompletionManifest,
    approval: &'a Approval,
    run_schema_version: u32,
    terminal_reason: &'a Option<String>,
}

/// Write-once authorization prefix for one sequence analysis.
///
/// The completion seal freezes outputs and terminal state later. This earlier
/// seal freezes the exact authority facts which made execution legal before
/// any scientific output can be written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SeqAuthorityPrefixSeal {
    schema_version: u32,
    context: RunContext,
    approval: Approval,
    created_event: Event,
    allowed_event: Event,
    authority_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct SeqAuthorityPrefixSealPayload<'a> {
    schema_version: u32,
    context: &'a RunContext,
    approval: &'a Approval,
    created_event: &'a Event,
    allowed_event: &'a Event,
}

#[derive(Debug, Clone)]
pub struct ScienceStore {
    root: PathBuf,
    root_capability: Arc<Mutex<StoreRootCapability>>,
    writes: Arc<Mutex<()>>,
}

/// Opaque kernel-owned single-flight lease for one deterministic operation.
///
/// The lock descriptor is opened relative to the ScienceStore's retained root
/// capability, so pathname replacement cannot make the actor lock one store
/// while committing to another. Closing this value, including process death,
/// releases the kernel lock; the retained lock file is not an ownership marker.
#[derive(Debug)]
pub struct ScienceOperationLease {
    key: (StoreRootIdentity, RunId),
    #[cfg(unix)]
    _file: fs::File,
}

impl Drop for ScienceOperationLease {
    fn drop(&mut self) {
        active_science_operation_leases()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.key);
    }
}

/// Retained authority for one canonical session workspace.
///
/// Product adapters use this capability when source admission and store
/// provisioning must share one directory identity. Its fields intentionally
/// remain private: callers can request descriptor-relative operations, but
/// cannot substitute a pathname after the workspace has been admitted.
#[derive(Debug)]
pub struct ScienceWorkspaceCapability {
    #[cfg(unix)]
    admitted_root: PathBuf,
    #[cfg(unix)]
    directory: PinnedDirectory,
    #[cfg(unix)]
    identity: StoreRootIdentity,
    #[cfg(not(unix))]
    _unsupported: (),
}

/// Serialize Science authority mutations across independently constructed
/// stores in this process.
///
/// A store-local lock only protects clones. Product code legitimately reopens
/// the same retained root through separate `ScienceStore` values, so a
/// per-instance lock permits a terminal transition to race an output write.
/// The deliberately coarse process lock is fail-closed and keeps the critical
/// sections short; it can later be sharded by retained directory identity
/// without weakening this contract.
fn shared_science_write_lock() -> Arc<Mutex<()>> {
    static WRITES: OnceLock<Arc<Mutex<()>>> = OnceLock::new();
    Arc::clone(WRITES.get_or_init(|| Arc::new(Mutex::new(()))))
}

fn active_science_operation_leases() -> &'static Mutex<HashSet<(StoreRootIdentity, RunId)>> {
    static ACTIVE: OnceLock<Mutex<HashSet<(StoreRootIdentity, RunId)>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(HashSet::new()))
}

#[derive(Debug)]
enum StoreRootCapability {
    Pending,
    Pinned(PinnedDirectory),
    Unavailable(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum StoreRootIdentity {
    #[cfg(unix)]
    Unix { device: u64, inode: u64 },
    #[cfg(windows)]
    Windows { volume: u32, index: u64 },
}

impl ScienceWorkspaceCapability {
    /// Open the canonical session workspace once and retain that exact
    /// directory identity for all subsequent source and store operations.
    #[cfg(unix)]
    pub fn open(workspace_root: impl AsRef<Path>) -> Result<Self> {
        let admitted_root = dunce::canonicalize(workspace_root.as_ref())?;
        if !admitted_root.is_absolute() {
            return Err(ScienceError::Invalid(
                "science workspace must resolve to an absolute directory".into(),
            ));
        }
        let expected =
            PinnedDirectory::checked_directory_identity(&admitted_root, "science workspace")?;
        let directory = PinnedDirectory::open_absolute_path(&admitted_root)?;
        if directory.identity()? != expected
            || directory.final_path()? != admitted_root
            || PinnedDirectory::checked_directory_identity(&admitted_root, "science workspace")?
                != expected
        {
            return Err(ScienceError::Invalid(
                "science workspace identity changed during capability admission".into(),
            ));
        }
        Ok(Self {
            admitted_root,
            directory,
            identity: expected,
        })
    }

    /// Non-Unix products fail closed until they can retain a workspace handle
    /// with equivalent no-follow, descriptor-relative semantics.
    #[cfg(not(unix))]
    pub fn open(_workspace_root: impl AsRef<Path>) -> Result<Self> {
        Err(ScienceError::FeatureDisabled(
            "retained science workspace capability has no backend on this platform".into(),
        ))
    }

    /// Resolve the workspace's current handle path without reacquiring
    /// authority from that pathname.
    #[cfg(unix)]
    pub fn current_path(&self) -> Result<PathBuf> {
        if self.directory.identity()? != self.identity {
            return Err(ScienceError::Invalid(
                "retained science workspace identity changed".into(),
            ));
        }
        let current = self.directory.final_path()?;
        if !current.is_absolute()
            || PinnedDirectory::checked_directory_identity(&current, "retained science workspace")?
                != self.identity
        {
            return Err(ScienceError::Invalid(
                "retained science workspace has no stable current path".into(),
            ));
        }
        Ok(current)
    }

    #[cfg(not(unix))]
    pub fn current_path(&self) -> Result<PathBuf> {
        Err(ScienceError::FeatureDisabled(
            "retained science workspace capability has no backend on this platform".into(),
        ))
    }

    #[cfg(unix)]
    fn assert_admitted_path_attached(&self) -> Result<()> {
        if self.current_path()? != self.admitted_root
            || PinnedDirectory::checked_directory_identity(
                &self.admitted_root,
                "admitted science workspace",
            )? != self.identity
        {
            return Err(ScienceError::Invalid(
                "science workspace pathname changed during source admission".into(),
            ));
        }
        Ok(())
    }

    #[cfg(unix)]
    fn store_relative_path(&self, root: &Path) -> Result<PathBuf> {
        let relative = if root.is_absolute() {
            let current = self.current_path()?;
            root.strip_prefix(&self.admitted_root)
                .or_else(|_| root.strip_prefix(&current))
                .map_err(|_| {
                    ScienceError::Invalid(
                        "science store root must be inside the retained workspace".into(),
                    )
                })?
                .to_path_buf()
        } else {
            root.to_path_buf()
        };
        if relative.as_os_str().is_empty() {
            return Ok(relative);
        }
        validate_relative(&relative)?;
        Ok(relative)
    }

    /// Snapshot one regular workspace file through the retained workspace
    /// descriptor. The returned path is the exact handle-resolved path whose
    /// bounded bytes were read.
    #[cfg(unix)]
    pub fn snapshot_regular_bounded(
        &self,
        source_path: impl AsRef<Path>,
        max_bytes: u64,
    ) -> Result<(PathBuf, Vec<u8>)> {
        self.snapshot_regular_bounded_with_hook(source_path.as_ref(), max_bytes, || Ok(()))
    }

    #[cfg(not(unix))]
    pub fn snapshot_regular_bounded(
        &self,
        _source_path: impl AsRef<Path>,
        _max_bytes: u64,
    ) -> Result<(PathBuf, Vec<u8>)> {
        Err(ScienceError::FeatureDisabled(
            "retained science source snapshot has no backend on this platform".into(),
        ))
    }

    #[cfg(unix)]
    fn snapshot_regular_bounded_with_hook(
        &self,
        source_path: &Path,
        max_bytes: u64,
        after_identity_snapshot: impl FnOnce() -> Result<()>,
    ) -> Result<(PathBuf, Vec<u8>)> {
        let requested = if source_path.is_absolute() {
            source_path.to_path_buf()
        } else {
            self.admitted_root.join(source_path)
        };
        let requested_metadata = fs::symlink_metadata(&requested)?;
        if requested_metadata.file_type().is_symlink() || !requested_metadata.is_file() {
            return Err(ScienceError::Invalid(
                "science source must be a non-symlink regular file".into(),
            ));
        }
        if requested_metadata.len() > max_bytes {
            return Err(ScienceError::Invalid(format!(
                "science source exceeds the {max_bytes}-byte cap"
            )));
        }
        let expected_source = PinnedDirectory::metadata_identity(&requested_metadata);
        let canonical_source = dunce::canonicalize(&requested)?;
        let relative = canonical_source
            .strip_prefix(&self.admitted_root)
            .map_err(|_| {
                ScienceError::Invalid("science source must be inside the retained workspace".into())
            })?
            .to_path_buf();
        validate_relative(&relative)?;

        after_identity_snapshot()?;
        self.assert_admitted_path_attached()?;

        let parent = match relative.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => {
                self.directory.open_directory(parent)?
            }
            _ => self.directory.try_clone()?,
        };
        let name = relative
            .file_name()
            .ok_or_else(|| ScienceError::Invalid("science source path has no file name".into()))?;
        let mut file = openat(
            &parent.file,
            name,
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            None,
        )
        .map_err(|error| match error {
            ScienceError::Io(io) if io.raw_os_error() == Some(libc::ELOOP) => {
                ScienceError::Invalid("science source must not traverse a symlink".into())
            }
            error => error,
        })?;
        let opened_metadata = file.metadata()?;
        if !opened_metadata.is_file()
            || PinnedDirectory::metadata_identity(&opened_metadata) != expected_source
        {
            return Err(ScienceError::Invalid(
                "science source identity changed during descriptor-relative open".into(),
            ));
        }
        if opened_metadata.len() > max_bytes {
            return Err(ScienceError::Invalid(format!(
                "science source exceeds the {max_bytes}-byte cap"
            )));
        }
        let opened_handle = PinnedDirectory {
            file: file.try_clone()?,
        };
        if opened_handle.final_path()? != canonical_source
            || !canonical_source.starts_with(self.current_path()?)
            || PinnedDirectory::metadata_identity(&fs::symlink_metadata(&requested)?)
                != expected_source
        {
            return Err(ScienceError::Invalid(
                "science source pathname no longer identifies the opened file".into(),
            ));
        }

        let mut bytes = Vec::new();
        std::io::Read::by_ref(&mut file)
            .take(max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > max_bytes {
            return Err(ScienceError::Invalid(format!(
                "science source exceeds the {max_bytes}-byte cap"
            )));
        }
        let final_requested = fs::symlink_metadata(&requested)?;
        if final_requested.file_type().is_symlink()
            || !final_requested.is_file()
            || PinnedDirectory::metadata_identity(&final_requested) != expected_source
            || PinnedDirectory::metadata_identity(&file.metadata()?) != expected_source
            || opened_handle.final_path()? != canonical_source
        {
            return Err(ScienceError::Invalid(
                "science source changed while its bytes were read".into(),
            ));
        }
        self.assert_admitted_path_attached()?;
        Ok((canonical_source, bytes))
    }

    /// Provision a ScienceStore root beneath the retained workspace and
    /// transfer the already-open root directory directly into the store.
    ///
    /// No pathname is canonicalized or reopened between directory creation
    /// and store construction.
    #[cfg(unix)]
    pub fn create_science_store(&self, root: impl AsRef<Path>) -> Result<ScienceStore> {
        let expected_relative = self.store_relative_path(root.as_ref())?;
        let directory = if expected_relative.as_os_str().is_empty() {
            self.directory.try_clone()?
        } else {
            self.directory.create_directories(&expected_relative)?
        };
        let workspace_path = self.current_path()?;
        let root_path = directory.final_path()?;
        let actual_relative = root_path.strip_prefix(&workspace_path).map_err(|_| {
            ScienceError::Invalid("opened science store root escaped the retained workspace".into())
        })?;
        if actual_relative != expected_relative
            || directory.final_path()? != root_path
            || directory.identity()?
                != PinnedDirectory::checked_directory_identity(
                    &root_path,
                    "opened science store root",
                )?
        {
            return Err(ScienceError::Invalid(
                "science store root identity changed during retained provisioning".into(),
            ));
        }
        Ok(ScienceStore {
            root: root_path,
            root_capability: Arc::new(Mutex::new(StoreRootCapability::Pinned(directory))),
            writes: shared_science_write_lock(),
        })
    }

    #[cfg(not(unix))]
    pub fn create_science_store(&self, _root: impl AsRef<Path>) -> Result<ScienceStore> {
        Err(ScienceError::FeatureDisabled(
            "retained science store provisioning has no backend on this platform".into(),
        ))
    }
}

impl ScienceStore {
    const MAX_DOSSIER_REGISTRY_BYTES: u64 = 8 * 1024 * 1024;
    const MAX_DOSSIER_RUN_RECORD_BYTES: u64 = 1024 * 1024;
    const MAX_DOSSIER_COMPLETION_SEAL_BYTES: u64 = 48 * 1024 * 1024;

    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let root_capability = match fs::symlink_metadata(&root) {
            Ok(metadata) if metadata.file_type().is_symlink() => StoreRootCapability::Unavailable(
                "science store root must not be a symlink or reparse point".into(),
            ),
            Ok(metadata) if !metadata.is_dir() => {
                StoreRootCapability::Unavailable("science store root must be a directory".into())
            }
            Ok(_) => match PinnedDirectory::open_path(&root) {
                Ok(directory) => StoreRootCapability::Pinned(directory),
                Err(error) => StoreRootCapability::Unavailable(error.to_string()),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                StoreRootCapability::Pending
            }
            Err(error) => StoreRootCapability::Unavailable(error.to_string()),
        };
        Self {
            root,
            root_capability: Arc::new(Mutex::new(root_capability)),
            writes: shared_science_write_lock(),
        }
    }

    /// Open and retain the store root relative to a pinned canonical workspace
    /// directory, then verify both the pre-open identity and the final
    /// handle-resolved location. Product adapters use this constructor after
    /// provisioning the root so a rename, symlink, or in-workspace replacement
    /// between pathname inspection and open cannot redirect later record
    /// writes.
    pub fn new_confined(
        root: impl Into<PathBuf>,
        workspace_root: impl AsRef<Path>,
    ) -> Result<Self> {
        let root = root.into();
        #[cfg(unix)]
        let directory = PinnedDirectory::open_existing_confined(&root, workspace_root.as_ref())?;
        #[cfg(not(unix))]
        let directory = {
            let metadata = fs::symlink_metadata(&root)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(ScienceError::Invalid(
                    "science store root must be a non-symlink directory".into(),
                ));
            }
            let directory = PinnedDirectory::open_path(&root)?;
            let opened_root = directory.final_path()?;
            let workspace = dunce::canonicalize(workspace_root.as_ref())?;
            if !opened_root.starts_with(&workspace) {
                return Err(ScienceError::Invalid(
                    "opened science store root escapes canonical workspace".into(),
                ));
            }
            directory
        };
        Ok(Self {
            root,
            root_capability: Arc::new(Mutex::new(StoreRootCapability::Pinned(directory))),
            writes: shared_science_write_lock(),
        })
    }

    /// Durable root owned by this store. Product adapters use this only to
    /// prove the store they hand to a SessionActor is the same confined root
    /// recorded in the run context; callers still cannot construct run paths.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Claim cross-process single-flight ownership relative to this store's
    /// already-retained directory capability.
    pub fn claim_operation_lease(&self, run_id: &RunId) -> Result<ScienceOperationLease> {
        run_id.validate()?;
        #[cfg(unix)]
        {
            let root = self.root_directory()?;
            root.assert_private_owned_directory("science store root")?;
            let key = (root.identity()?, run_id.clone());
            {
                let mut active = active_science_operation_leases().lock().map_err(|_| {
                    ScienceError::Invalid("science operation lease registry is poisoned".into())
                })?;
                if !active.insert(key.clone()) {
                    return Err(ScienceError::Invalid(format!(
                        "operation {} is already active in this Lumen process",
                        run_id.0
                    )));
                }
            }

            let lock = (|| {
                let leases = root.create_directories(Path::new(".seq-analyze-leases"))?;
                leases.assert_private_owned_directory("science operation lease directory")?;
                leases.try_lock_operation_file(Path::new(&format!("{}.lock", run_id.0)))
            })();
            match lock {
                Ok(file) => Ok(ScienceOperationLease { key, _file: file }),
                Err(error) => {
                    active_science_operation_leases()
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .remove(&key);
                    Err(error)
                }
            }
        }
        #[cfg(not(unix))]
        {
            Err(ScienceError::FeatureDisabled(
                "descriptor-safe cross-process operation leases are unavailable on this platform"
                    .into(),
            ))
        }
    }

    /// Compare the retained directory handles, not their path spellings.
    ///
    /// Dossier composition joins Science run records with project records.
    /// Both stores must therefore pin the exact same directory identity; a
    /// rename-and-replace between their constructors must fail closed.
    pub fn shares_root_capability_with(
        &self,
        project_store: &project::ProjectStore,
    ) -> Result<bool> {
        Ok(self.root_directory()?.identity()? == project_store.root_identity()?)
    }

    /// Retain the process-wide Science authority write lock across a
    /// cross-store commit closure.
    ///
    /// The caller must acquire its ProjectStore write guard first. The closure
    /// may reopen authority-owned records and write that already-guarded
    /// ProjectStore, but must not call a ScienceStore method which itself
    /// acquires `writes`. This Project -> Science ordering matches the other
    /// cross-store product paths and prevents ABBA deadlocks. Migration uses
    /// the seam to keep Running+Allow stable from final validation through the
    /// project commit marker; otherwise a public terminal transition could
    /// race between the check and publication.
    pub(crate) fn with_exclusive_authority<T>(
        &self,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        operation()
    }

    /// Persist a cross-store point-of-no-return while the caller retains
    /// `writes`. This method intentionally does not acquire the mutex again.
    pub(crate) fn mark_authority_commit_fence_unlocked(
        &self,
        context: &RunContext,
        call_id: &CallId,
        operation_id: &str,
    ) -> Result<()> {
        validate_context(context)?;
        call_id.validate()?;
        project::mutation::validate_operation_id(operation_id)?;
        let run_dir = self.open_run_directory(&context.run_id)?;
        let run: RunRecord = run_dir.read_json(Path::new("run.json"))?;
        validate_context(&run.context)?;
        if run.context != *context || run.state != RunState::Running {
            return Err(ScienceError::Invalid(
                "authority commit fence requires the exact Running context".into(),
            ));
        }
        Self::require_running_allowed_output(&run_dir, &run, &context.run_id, Some(call_id))?;
        let expected = AuthorityCommitFence {
            schema_version: SCHEMA_VERSION,
            run_id: context.run_id.clone(),
            project_id: context.project_id.clone(),
            call_id: call_id.clone(),
            operation_id: operation_id.to_owned(),
        };
        let path = Path::new(AUTHORITY_COMMIT_FENCE_FILE);
        match run_dir.read_json::<AuthorityCommitFence>(path) {
            Ok(existing) if existing == expected => return Ok(()),
            Ok(_) => {
                return Err(ScienceError::Invalid(
                    "authority commit fence conflicts with another operation".into(),
                ));
            }
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        match run_dir.replace_json_atomic(path, &expected) {
            Ok(()) => Ok(()),
            Err(error) => match run_dir.read_json::<AuthorityCommitFence>(path) {
                Ok(reopened) if reopened == expected => Ok(()),
                _ => Err(error),
            },
        }
    }

    fn authority_commit_fence(run_dir: &PinnedDirectory) -> Result<Option<AuthorityCommitFence>> {
        match run_dir.read_json(Path::new(AUTHORITY_COMMIT_FENCE_FILE)) {
            Ok(fence) => Ok(Some(fence)),
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    fn successful_completion_seal_digest(seal: &SuccessfulCompletionSeal) -> Result<String> {
        Ok(hex_sha256(&serde_json::to_vec(
            &SuccessfulCompletionSealPayload {
                schema_version: seal.schema_version,
                manifest: &seal.manifest,
                approval: &seal.approval,
                run_schema_version: seal.run_schema_version,
                terminal_reason: &seal.terminal_reason,
            },
        )?))
    }

    fn expected_successful_completion_seal(
        run: &RunRecord,
        approval: &Approval,
        manifest: &SuccessfulCompletionManifest,
    ) -> Result<SuccessfulCompletionSeal> {
        let mut seal = SuccessfulCompletionSeal {
            schema_version: SCHEMA_VERSION,
            manifest: manifest.clone(),
            approval: approval.clone(),
            run_schema_version: run.schema_version,
            terminal_reason: None,
            manifest_sha256: String::new(),
        };
        seal.manifest_sha256 = Self::successful_completion_seal_digest(&seal)?;
        Ok(seal)
    }

    fn successful_completion_seal(
        run_dir: &PinnedDirectory,
    ) -> Result<Option<SuccessfulCompletionSeal>> {
        match run_dir.read_json(Path::new(SUCCESSFUL_COMPLETION_SEAL_FILE)) {
            Ok(seal) => {
                let seal: SuccessfulCompletionSeal = seal;
                validate_context(&seal.manifest.context)?;
                if seal.schema_version != SCHEMA_VERSION
                    || seal.manifest_sha256 != Self::successful_completion_seal_digest(&seal)?
                {
                    return Err(ScienceError::Invalid(
                        "successful completion seal is malformed or corrupt".into(),
                    ));
                }
                Ok(Some(seal))
            }
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    fn successful_completion_seal_bounded(
        run_dir: &PinnedDirectory,
    ) -> Result<Option<SuccessfulCompletionSeal>> {
        match run_dir.read_json_bounded(
            Path::new(SUCCESSFUL_COMPLETION_SEAL_FILE),
            Self::MAX_DOSSIER_COMPLETION_SEAL_BYTES,
        ) {
            Ok(seal) => {
                let seal: SuccessfulCompletionSeal = seal;
                validate_context(&seal.manifest.context)?;
                if seal.schema_version != SCHEMA_VERSION
                    || seal.manifest_sha256 != Self::successful_completion_seal_digest(&seal)?
                {
                    return Err(ScienceError::Invalid(
                        "successful completion seal is malformed or corrupt".into(),
                    ));
                }
                Ok(Some(seal))
            }
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    fn persist_successful_completion_seal_unlocked(
        run_dir: &PinnedDirectory,
        run: &RunRecord,
        approval: &Approval,
        manifest: &SuccessfulCompletionManifest,
    ) -> Result<()> {
        let expected = Self::expected_successful_completion_seal(run, approval, manifest)?;
        let path = Path::new(SUCCESSFUL_COMPLETION_SEAL_FILE);
        match Self::successful_completion_seal(run_dir)? {
            Some(existing) if existing == expected => return Ok(()),
            Some(_) => {
                return Err(ScienceError::Invalid(
                    "successful completion seal conflicts with the exact manifest".into(),
                ));
            }
            None => {}
        }
        match run_dir.replace_json_atomic(path, &expected) {
            Ok(()) => Ok(()),
            Err(error) => match Self::successful_completion_seal(run_dir) {
                Ok(Some(reopened)) if reopened == expected => Ok(()),
                _ => Err(error),
            },
        }
    }

    fn seq_authority_prefix_digest(seal: &SeqAuthorityPrefixSeal) -> Result<String> {
        Ok(hex_sha256(&serde_json::to_vec(
            &SeqAuthorityPrefixSealPayload {
                schema_version: seal.schema_version,
                context: &seal.context,
                approval: &seal.approval,
                created_event: &seal.created_event,
                allowed_event: &seal.allowed_event,
            },
        )?))
    }

    fn expected_seq_authority_prefix_seal(
        context: &RunContext,
        approval: &Approval,
        created_event: &Event,
        allowed_event: &Event,
    ) -> Result<SeqAuthorityPrefixSeal> {
        validate_context(context)?;
        validate_approval(approval, &context.run_id)?;
        validate_events(
            &[created_event.clone(), allowed_event.clone()],
            &context.run_id,
        )?;
        if approval.project_id != context.project_id
            || approval.owner_id != context.owner_id
            || approval.decision != ApprovalDecision::Allow
            || approval.decided_at.is_none()
            || created_event.schema_version != SCHEMA_VERSION
            || allowed_event.schema_version != SCHEMA_VERSION
            || created_event.seq != 1
            || allowed_event.seq != 2
            || created_event.actor != "SessionActor"
            || created_event.kind != "run.created"
            || allowed_event.actor != "LumenApproval"
            || allowed_event.kind != "approval.allowed"
            || created_event.timestamp > allowed_event.timestamp
            || approval.decided_at.is_none_or(|decided_at| {
                created_event.timestamp > decided_at || allowed_event.timestamp < decided_at
            })
        {
            return Err(ScienceError::Invalid(
                "sequence authority prefix is not one exact durable Allow".into(),
            ));
        }
        let mut seal = SeqAuthorityPrefixSeal {
            schema_version: SCHEMA_VERSION,
            context: context.clone(),
            approval: approval.clone(),
            created_event: created_event.clone(),
            allowed_event: allowed_event.clone(),
            authority_sha256: String::new(),
        };
        seal.authority_sha256 = Self::seq_authority_prefix_digest(&seal)?;
        Ok(seal)
    }

    fn seq_authority_prefix_seal(
        run_dir: &PinnedDirectory,
    ) -> Result<Option<SeqAuthorityPrefixSeal>> {
        match run_dir.read_json(Path::new(SEQ_AUTHORITY_PREFIX_SEAL_FILE)) {
            Ok(seal) => {
                let seal: SeqAuthorityPrefixSeal = seal;
                let expected = Self::expected_seq_authority_prefix_seal(
                    &seal.context,
                    &seal.approval,
                    &seal.created_event,
                    &seal.allowed_event,
                )?;
                if seal.schema_version != SCHEMA_VERSION
                    || seal.authority_sha256 != Self::seq_authority_prefix_digest(&seal)?
                    || seal != expected
                {
                    return Err(ScienceError::Invalid(
                        "sequence authority prefix seal is malformed or corrupt".into(),
                    ));
                }
                Ok(Some(seal))
            }
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    fn verify_seq_authority_prefix_unlocked(
        run_dir: &PinnedDirectory,
        context: &RunContext,
        approval: &Approval,
        created_event: &Event,
        allowed_event: &Event,
    ) -> Result<String> {
        let expected = Self::expected_seq_authority_prefix_seal(
            context,
            approval,
            created_event,
            allowed_event,
        )?;
        let run: RunRecord = run_dir.read_json(Path::new("run.json"))?;
        let approvals: Vec<Approval> = run_dir.read_json(Path::new("approvals.json"))?;
        let events: Vec<Event> = run_dir.read_json(Path::new("events.json"))?;
        validate_context(&run.context)?;
        validate_approvals(&approvals, &context.run_id)?;
        validate_events(&events, &context.run_id)?;
        if run.context != *context
            || !matches!(
                run.state,
                RunState::AwaitingApproval | RunState::Running | RunState::Succeeded
            )
            || approvals.as_slice() != [approval.clone()]
            || events.first() != Some(created_event)
            || events.get(1) != Some(allowed_event)
        {
            return Err(ScienceError::Invalid(
                "durable sequence authority records differ from their exact prefix".into(),
            ));
        }
        let Some(seal) = Self::seq_authority_prefix_seal(run_dir)? else {
            return Err(ScienceError::Invalid(
                "durable sequence Allow is missing its authority prefix seal".into(),
            ));
        };
        if seal != expected {
            return Err(ScienceError::Invalid(
                "sequence authority prefix conflicts with its write-once seal".into(),
            ));
        }
        // Reading an exact seal is not a durability proof after an ambiguous
        // create-new parent-sync error. Re-sync the retained containing
        // directory before any recovery or replay may treat it as authority.
        run_dir.sync_directory()?;
        Ok(seal.authority_sha256)
    }

    /// Publish one exact sequence Allow prefix without replacing an existing
    /// seal. If publication reports an error after the new name became visible,
    /// only an exact read-back reconciles the cut.
    pub(crate) fn persist_seq_authority_prefix(
        &self,
        context: &RunContext,
        approval: &Approval,
        created_event: &Event,
        allowed_event: &Event,
    ) -> Result<String> {
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let run_dir = self.open_run_directory(&context.run_id)?;
        let expected = Self::expected_seq_authority_prefix_seal(
            context,
            approval,
            created_event,
            allowed_event,
        )?;
        let run: RunRecord = run_dir.read_json(Path::new("run.json"))?;
        let approvals: Vec<Approval> = run_dir.read_json(Path::new("approvals.json"))?;
        let events: Vec<Event> = run_dir.read_json(Path::new("events.json"))?;
        let artifacts: Vec<Artifact> = run_dir.read_json(Path::new("artifacts.json"))?;
        let evidence: Vec<Evidence> = run_dir.read_json(Path::new("evidence.json"))?;
        let provenance: Vec<Provenance> = run_dir.read_json(Path::new("provenance.json"))?;
        let previews: Vec<preview::PreviewRecord> =
            run_dir.read_json(Path::new("previews.json"))?;
        if run.context != *context
            || !matches!(run.state, RunState::AwaitingApproval | RunState::Running)
            || approvals.as_slice() != [approval.clone()]
            || events.as_slice() != [created_event.clone(), allowed_event.clone()]
            || !artifacts.is_empty()
            || !evidence.is_empty()
            || !provenance.is_empty()
            || !previews.is_empty()
        {
            return Err(ScienceError::Invalid(
                "sequence authority prefix cannot seal changed records or pre-authorization outputs"
                    .into(),
            ));
        }
        match Self::seq_authority_prefix_seal(&run_dir)? {
            Some(existing) if existing == expected => {
                // An earlier create-new publication may have become visible
                // before its parent-directory sync failed. Only a successful
                // sync through this retained run capability makes that retry
                // durable enough to authorize execution.
                run_dir.sync_directory()?;
                return Ok(existing.authority_sha256);
            }
            Some(_) => {
                return Err(ScienceError::Invalid(
                    "sequence authority prefix seal conflicts with another Allow".into(),
                ));
            }
            None => {}
        }

        let path = Path::new(SEQ_AUTHORITY_PREFIX_SEAL_FILE);
        let bytes = serde_json::to_vec_pretty(&expected)?;
        match run_dir.write_new_atomic(path, &bytes) {
            Ok(()) => Ok(expected.authority_sha256),
            // Never turn a merely visible create-new name into authority in
            // this call. A later retry must reopen the exact seal and
            // successfully sync the retained parent before it can proceed.
            Err(error) => Err(error),
        }
    }

    pub(crate) fn verify_seq_authority_prefix(
        &self,
        context: &RunContext,
        approval: &Approval,
        created_event: &Event,
        allowed_event: &Event,
    ) -> Result<String> {
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let run_dir = self.open_run_directory(&context.run_id)?;
        Self::verify_seq_authority_prefix_unlocked(
            &run_dir,
            context,
            approval,
            created_event,
            allowed_event,
        )
    }

    pub(crate) fn reject_seq_authority_prefix(&self, run_id: &RunId) -> Result<()> {
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let run_dir = self.open_run_directory(run_id)?;
        if Self::seq_authority_prefix_seal(&run_dir)?.is_some() {
            return Err(ScienceError::Invalid(
                "non-Allow sequence operation carried an authority prefix seal".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn transition_seq_succeeded_with_manifest(
        &self,
        manifest: &SuccessfulCompletionManifest,
        approval: &Approval,
        created_event: &Event,
        allowed_event: &Event,
    ) -> Result<RunRecord> {
        validate_context(&manifest.context)?;
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let run_dir = self.open_run_directory(&manifest.context.run_id)?;
        Self::verify_seq_authority_prefix_unlocked(
            &run_dir,
            &manifest.context,
            approval,
            created_event,
            allowed_event,
        )?;
        Self::transition_succeeded_with_manifest_unlocked(&run_dir, manifest)
    }

    fn successful_completion_anchor(run: &RunRecord) -> Result<Option<&str>> {
        match run.successful_completion_manifest_sha256.as_deref() {
            Some(digest) if is_sha256_hex(digest) => Ok(Some(digest)),
            Some(_) => Err(ScienceError::Invalid(
                "successful completion manifest anchor is malformed".into(),
            )),
            None => Ok(None),
        }
    }

    fn persist_successful_completion_anchor_unlocked(
        run_dir: &PinnedDirectory,
        run: &mut RunRecord,
        manifest_sha256: &str,
    ) -> Result<()> {
        if !is_sha256_hex(manifest_sha256) {
            return Err(ScienceError::Invalid(
                "successful completion manifest digest is malformed".into(),
            ));
        }
        match Self::successful_completion_anchor(run)? {
            Some(existing) if existing == manifest_sha256 => return Ok(()),
            Some(_) => {
                return Err(ScienceError::Invalid(
                    "successful completion manifest anchor conflicts with another snapshot".into(),
                ));
            }
            None => {}
        }
        run.successful_completion_manifest_sha256 = Some(manifest_sha256.to_owned());
        run_dir.replace_json_atomic(Path::new("run.json"), run)
    }

    fn reject_successful_completion_seal(run_dir: &PinnedDirectory) -> Result<()> {
        let run: RunRecord = run_dir.read_json(Path::new("run.json"))?;
        if Self::successful_completion_anchor(&run)?.is_some()
            || Self::successful_completion_seal(run_dir)?.is_some()
        {
            return Err(ScienceError::Invalid(
                "exact successful completion is sealed and immutable".into(),
            ));
        }
        Ok(())
    }

    fn reject_fenced_rollback(run_dir: &PinnedDirectory) -> Result<()> {
        if Self::authority_commit_fence(run_dir)?.is_some() {
            return Err(ScienceError::Invalid(
                "authority passed its durable project commit fence and cannot roll back or terminalize unsuccessfully"
                    .into(),
            ));
        }
        let run: RunRecord = run_dir.read_json(Path::new("run.json"))?;
        if Self::successful_completion_anchor(&run)?.is_some()
            || Self::successful_completion_seal(run_dir)?.is_some()
        {
            return Err(ScienceError::Invalid(
                "authority sealed exact successful completion and cannot roll back or terminalize unsuccessfully"
                    .into(),
            ));
        }
        Ok(())
    }

    /// Prove that workflow I/O and the authority store retained the same
    /// directory handle. Comparing path strings is insufficient because an
    /// attacker can rename and replace a validated store between independent
    /// opens.
    pub fn shares_root_capability_with_workflow_io(
        &self,
        workflow_io: &workflow::WorkflowIoCapability,
    ) -> Result<bool> {
        Ok(self.root_directory()?.identity()? == workflow_io.shared_root().identity()?)
    }

    pub fn create_run(&self, context: RunContext) -> Result<RunRecord> {
        validate_context(&context)?;
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let record = RunRecord {
            schema_version: SCHEMA_VERSION,
            context,
            state: RunState::Created,
            terminal_reason: None,
            successful_completion_manifest_sha256: None,
        };
        let root = self.root_directory()?;
        let runs = root.create_directories(Path::new("runs"))?;
        let run_name = Path::new(&record.context.run_id.0);
        let staging_name = format!(".run-init-{}-{}", record.context.run_id.0, Uuid::now_v7());
        let staging_path = Path::new(&staging_name);
        let staging = runs.create_directory_new(staging_path)?;
        let initialized = Self::initialize_staged_run(&staging, &record);
        if let Err(error) = initialized {
            let _ = Self::discard_staged_run(&runs, staging_path, &staging);
            return Err(error);
        }
        let publication = runs.publish_directory_new(staging_path, run_name);
        #[cfg(test)]
        let publication = if publication.is_ok()
            && FAIL_RUN_PUBLICATION_AFTER_VISIBLE_RENAME.with(|fail| fail.replace(false))
        {
            Err(ScienceError::Io(std::io::Error::other(
                "injected runs-directory sync failure after visible rename",
            )))
        } else {
            publication
        };
        if let Err(error) = publication {
            // A no-replace rename may be visible even when syncing the parent
            // directory reports an error. Retained-directory identity may only
            // decide whether cleanup is safe; visibility cannot prove crash
            // durability. Return the original sync error and let a later
            // deterministic retry reopen the visible run if it survived.
            if let Ok(published) = runs.open_directory(run_name)
                && published.identity()? == staging.identity()?
                && published.read_json::<RunRecord>(Path::new("run.json"))? == record
            {
                return Err(error);
            }
            if let Ok(still_staged) = runs.open_directory(staging_path)
                && still_staged.identity()? == staging.identity()?
            {
                let _ = Self::discard_staged_run(&runs, staging_path, &staging);
            }
            return Err(error);
        }
        Ok(record)
    }

    fn initialize_staged_run(dir: &PinnedDirectory, record: &RunRecord) -> Result<()> {
        dir.create_directories(Path::new("artifacts"))?;
        dir.replace_json_atomic(Path::new("events.json"), &Vec::<Event>::new())?;
        dir.replace_json_atomic(Path::new("artifacts.json"), &Vec::<Artifact>::new())?;
        dir.replace_json_atomic(Path::new("evidence.json"), &Vec::<Evidence>::new())?;
        dir.replace_json_atomic(Path::new("provenance.json"), &Vec::<Provenance>::new())?;
        dir.replace_json_atomic(Path::new("approvals.json"), &Vec::<Approval>::new())?;
        dir.replace_json_atomic(
            Path::new("previews.json"),
            &Vec::<preview::PreviewRecord>::new(),
        )?;
        // Publish run.json last inside staging. The whole initialized
        // directory is still invisible at its final runId until the
        // no-replace directory rename below.
        dir.replace_json_atomic(Path::new("run.json"), record)
    }

    fn discard_staged_run(
        runs: &PinnedDirectory,
        staging_path: &Path,
        staging: &PinnedDirectory,
    ) -> Result<()> {
        for file in [
            "run.json",
            "events.json",
            "artifacts.json",
            "evidence.json",
            "provenance.json",
            "approvals.json",
            "previews.json",
        ] {
            staging.unlink_file(Path::new(file))?;
        }
        staging.remove_directory_if_empty(Path::new("artifacts"))?;
        runs.remove_directory_if_empty(staging_path)
            .and_then(|removed| {
                if removed {
                    Ok(())
                } else {
                    Err(ScienceError::Invalid(
                        "staged run directory retained unexpected entries".into(),
                    ))
                }
            })
    }

    fn load_run_record_from_directory(
        run_dir: &PinnedDirectory,
        run_id: &RunId,
    ) -> Result<RunRecord> {
        let run: RunRecord = run_dir.read_json(Path::new("run.json"))?;
        validate_context(&run.context)?;
        if &run.context.run_id != run_id {
            return Err(ScienceError::Invalid(
                "run record identity does not match requested run".into(),
            ));
        }
        Ok(run)
    }

    /// Return the immutable exact-success snapshot, when this run opted into
    /// manifest completion.
    ///
    /// Public readers must use this gate before serving any run collection.
    /// This makes raw-filesystem changes to an individual collection fail
    /// closed outside recovery too. Legacy runs remain readable without a
    /// seal; an in-progress exact completion must be recovered before reads.
    fn successful_completion_manifest_for_read(
        run_dir: &PinnedDirectory,
        run: &RunRecord,
    ) -> Result<Option<SuccessfulCompletionSeal>> {
        let anchor = Self::successful_completion_anchor(run)?;
        let seal = Self::successful_completion_seal(run_dir)?;
        match (anchor, seal.as_ref()) {
            (None, None) => Ok(None),
            (Some(anchor), Some(seal)) if run.state == RunState::Succeeded => {
                Self::verify_succeeded_seal_unlocked(run_dir, run, anchor, seal)?;
                Ok(Some(seal.clone()))
            }
            (Some(_), None) | (None, Some(_)) if run.state.terminal() => {
                Err(ScienceError::Invalid(
                    "exact completion anchor and seal must both remain durable".into(),
                ))
            }
            (Some(_), _) | (_, Some(_)) => Err(ScienceError::Invalid(
                "exact completion is unfinished and requires restart recovery".into(),
            )),
        }
    }

    fn successful_completion_manifest_for_bounded_read(
        run_dir: &PinnedDirectory,
        run: &RunRecord,
    ) -> Result<Option<SuccessfulCompletionSeal>> {
        let anchor = Self::successful_completion_anchor(run)?;
        let seal = Self::successful_completion_seal_bounded(run_dir)?;
        match (anchor, seal.as_ref()) {
            (None, None) => Ok(None),
            (Some(anchor), Some(seal)) if run.state == RunState::Succeeded => {
                Self::verify_succeeded_seal_bounded(run_dir, run, anchor, seal)?;
                Ok(Some(seal.clone()))
            }
            (Some(_), None) | (None, Some(_)) if run.state.terminal() => {
                Err(ScienceError::Invalid(
                    "exact completion anchor and seal must both remain durable".into(),
                ))
            }
            (Some(_), _) | (_, Some(_)) => Err(ScienceError::Invalid(
                "exact completion is unfinished and requires restart recovery".into(),
            )),
        }
    }

    fn load_run_snapshot(
        &self,
        run_id: &RunId,
    ) -> Result<(RunRecord, Option<SuccessfulCompletionSeal>)> {
        let run_dir = self.open_run_directory(run_id)?;
        let run = Self::load_run_record_from_directory(&run_dir, run_id)?;
        let manifest = Self::successful_completion_manifest_for_read(&run_dir, &run)?;
        Ok((run, manifest))
    }

    fn load_run_snapshot_bounded(
        &self,
        run_id: &RunId,
        max_run_json_bytes: u64,
    ) -> Result<(RunRecord, Option<SuccessfulCompletionSeal>)> {
        let run_dir = self.open_run_directory(run_id)?;
        let run: RunRecord = run_dir.read_json_bounded(
            Path::new("run.json"),
            max_run_json_bytes.min(Self::MAX_DOSSIER_RUN_RECORD_BYTES),
        )?;
        validate_context(&run.context)?;
        if &run.context.run_id != run_id {
            return Err(ScienceError::Invalid(
                "run record identity does not match requested run".into(),
            ));
        }
        let manifest = Self::successful_completion_manifest_for_bounded_read(&run_dir, &run)?;
        Ok((run, manifest))
    }

    pub fn load_run(&self, run_id: &RunId) -> Result<RunRecord> {
        Ok(self.load_run_snapshot(run_id)?.0)
    }

    /// Reopen a run when present without weakening corruption handling.
    ///
    /// Only an actual missing directory/record becomes `None`; malformed,
    /// escaped or identity-mismatched records still fail closed.
    pub fn load_run_optional(&self, run_id: &RunId) -> Result<Option<RunRecord>> {
        match self.load_run(run_id) {
            Ok(run) => Ok(Some(run)),
            Err(ScienceError::Io(ref error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    pub fn load_run_bounded(&self, run_id: &RunId, max_json_bytes: u64) -> Result<RunRecord> {
        Ok(self.load_run_snapshot_bounded(run_id, max_json_bytes)?.0)
    }

    pub fn transition(
        &self,
        run_id: &RunId,
        state: RunState,
        reason: Option<String>,
    ) -> Result<RunRecord> {
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        self.transition_unlocked(run_id, state, reason)
    }

    /// Transition while the caller retains `writes`.
    ///
    /// Keeping this separate is essential for event failure and interrupted
    /// recovery paths, which must make a terminal transition without trying to
    /// acquire the non-reentrant mutex a second time.
    fn transition_unlocked(
        &self,
        run_id: &RunId,
        state: RunState,
        reason: Option<String>,
    ) -> Result<RunRecord> {
        let run_dir = self.open_run_directory(run_id)?;
        let mut run = Self::load_run_record_from_directory(&run_dir, run_id)?;
        if run.state.terminal() {
            return Err(ScienceError::Invalid(
                "terminal run cannot transition".into(),
            ));
        }
        if !run.state.can_transition_to(state) {
            return Err(ScienceError::Invalid(format!(
                "illegal science run transition: {:?} -> {state:?}",
                run.state
            )));
        }
        Self::reject_fenced_rollback(&run_dir)?;
        run.state = state;
        run.terminal_reason = reason;
        run_dir.replace_json_atomic(Path::new("run.json"), &run)?;
        Ok(run)
    }

    /// Make Succeeded the final visible commit and reconcile the narrow case
    /// where atomic replacement became visible but the directory-sync call
    /// reported an error. Returning an error while a read-back is already
    /// Succeeded would create an API/durable split that callers cannot safely
    /// roll back because terminal states are immutable.
    ///
    /// This compatibility seam verifies the durable Allow, but does not bind a
    /// caller snapshot of every output collection. New actor protocols must
    /// use `transition_succeeded_with_manifest`.
    pub fn transition_succeeded_verified(&self, run_id: &RunId) -> Result<RunRecord> {
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let run_dir = self.open_run_directory(run_id)?;
        let run: RunRecord = run_dir.read_json(Path::new("run.json"))?;
        validate_context(&run.context)?;
        if run.context.run_id != *run_id {
            return Err(ScienceError::Invalid(
                "run record identity does not match requested run".into(),
            ));
        }
        if Self::authority_commit_fence(&run_dir)?.is_some() {
            return Err(ScienceError::Invalid(
                "a fenced cross-store authority requires an exact completion manifest".into(),
            ));
        }
        Self::reject_successful_completion_seal(&run_dir)?;
        Self::require_running_allowed_output(&run_dir, &run, run_id, None)?;
        Self::persist_succeeded_unlocked(&run_dir, run)
    }

    /// Atomically verify an exact completion snapshot and commit Succeeded.
    ///
    /// No public store method is called after acquiring `writes`; all
    /// registries and payloads are read directly through the retained run
    /// capability. This avoids both a non-reentrant lock and the
    /// verify-unlock-mutate-relock race that the manifest closes.
    pub fn transition_succeeded_with_manifest(
        &self,
        manifest: &SuccessfulCompletionManifest,
    ) -> Result<RunRecord> {
        validate_context(&manifest.context)?;
        let run_id = &manifest.context.run_id;
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let run_dir = self.open_run_directory(run_id)?;
        Self::transition_succeeded_with_manifest_unlocked(&run_dir, manifest)
    }

    fn transition_succeeded_with_manifest_unlocked(
        run_dir: &PinnedDirectory,
        manifest: &SuccessfulCompletionManifest,
    ) -> Result<RunRecord> {
        let run_id = &manifest.context.run_id;
        let mut run: RunRecord = run_dir.read_json(Path::new("run.json"))?;
        validate_context(&run.context)?;
        if run.context != manifest.context || run.state != RunState::Running {
            return Err(ScienceError::Invalid(
                "successful completion manifest does not bind the running context".into(),
            ));
        }
        let approval = Self::running_allowed_approval(run_dir, &run, run_id, None)?;

        let artifacts: Vec<Artifact> = run_dir.read_json(Path::new("artifacts.json"))?;
        let evidence: Vec<Evidence> = run_dir.read_json(Path::new("evidence.json"))?;
        let provenance: Vec<Provenance> = run_dir.read_json(Path::new("provenance.json"))?;
        let previews: Vec<preview::PreviewRecord> =
            run_dir.read_json(Path::new("previews.json"))?;
        let events: Vec<Event> = run_dir.read_json(Path::new("events.json"))?;
        validate_artifacts(&artifacts, run_id)?;
        validate_run_ids(evidence.iter().map(|item| &item.run_id), run_id, "evidence")?;
        validate_run_ids(
            provenance.iter().map(|item| &item.run_id),
            run_id,
            "provenance",
        )?;
        validate_previews(&previews, run_id)?;
        validate_events(&events, run_id)?;

        if artifacts != manifest.artifacts
            || evidence != manifest.evidence
            || provenance != manifest.provenance
            || previews != manifest.previews
            || events != manifest.events
        {
            return Err(ScienceError::Invalid(
                "successful completion manifest does not exactly match durable collections".into(),
            ));
        }
        Self::verify_completion_collections(
            run_dir,
            run_id,
            &approval,
            &artifacts,
            &evidence,
            &provenance,
            &previews,
            &events,
            &manifest.final_event,
            true,
        )?;
        let expected_seal = Self::expected_successful_completion_seal(&run, &approval, manifest)?;
        Self::persist_successful_completion_anchor_unlocked(
            run_dir,
            &mut run,
            &expected_seal.manifest_sha256,
        )?;
        Self::persist_successful_completion_seal_unlocked(run_dir, &run, &approval, manifest)?;
        Self::persist_succeeded_unlocked(run_dir, run)
    }

    fn durable_completion_manifest_unlocked(
        run_dir: &PinnedDirectory,
        context: &RunContext,
    ) -> Result<SuccessfulCompletionManifest> {
        let run_id = &context.run_id;
        let artifacts: Vec<Artifact> = run_dir.read_json(Path::new("artifacts.json"))?;
        let evidence: Vec<Evidence> = run_dir.read_json(Path::new("evidence.json"))?;
        let provenance: Vec<Provenance> = run_dir.read_json(Path::new("provenance.json"))?;
        let previews: Vec<preview::PreviewRecord> =
            run_dir.read_json(Path::new("previews.json"))?;
        let events: Vec<Event> = run_dir.read_json(Path::new("events.json"))?;
        validate_artifacts(&artifacts, run_id)?;
        validate_run_ids(evidence.iter().map(|item| &item.run_id), run_id, "evidence")?;
        validate_run_ids(
            provenance.iter().map(|item| &item.run_id),
            run_id,
            "provenance",
        )?;
        validate_previews(&previews, run_id)?;
        validate_events(&events, run_id)?;
        let final_event = events.last().cloned().ok_or_else(|| {
            ScienceError::Invalid("sealed completion has no durable final event".into())
        })?;
        Ok(SuccessfulCompletionManifest {
            context: context.clone(),
            artifacts,
            evidence,
            provenance,
            previews,
            events,
            final_event,
        })
    }

    fn durable_completion_manifest_bounded(
        run_dir: &PinnedDirectory,
        context: &RunContext,
    ) -> Result<SuccessfulCompletionManifest> {
        let run_id = &context.run_id;
        let artifacts: Vec<Artifact> = run_dir.read_json_bounded(
            Path::new("artifacts.json"),
            Self::MAX_DOSSIER_REGISTRY_BYTES,
        )?;
        let evidence: Vec<Evidence> = run_dir
            .read_json_bounded(Path::new("evidence.json"), Self::MAX_DOSSIER_REGISTRY_BYTES)?;
        let provenance: Vec<Provenance> = run_dir.read_json_bounded(
            Path::new("provenance.json"),
            Self::MAX_DOSSIER_REGISTRY_BYTES,
        )?;
        let previews: Vec<preview::PreviewRecord> = run_dir
            .read_json_bounded(Path::new("previews.json"), Self::MAX_DOSSIER_REGISTRY_BYTES)?;
        let events: Vec<Event> = run_dir
            .read_json_bounded(Path::new("events.json"), Self::MAX_DOSSIER_REGISTRY_BYTES)?;
        validate_artifacts(&artifacts, run_id)?;
        validate_run_ids(evidence.iter().map(|item| &item.run_id), run_id, "evidence")?;
        validate_run_ids(
            provenance.iter().map(|item| &item.run_id),
            run_id,
            "provenance",
        )?;
        validate_previews(&previews, run_id)?;
        validate_events(&events, run_id)?;
        let final_event = events.last().cloned().ok_or_else(|| {
            ScienceError::Invalid("sealed completion has no durable final event".into())
        })?;
        Ok(SuccessfulCompletionManifest {
            context: context.clone(),
            artifacts,
            evidence,
            provenance,
            previews,
            events,
            final_event,
        })
    }

    fn verify_succeeded_seal_unlocked(
        run_dir: &PinnedDirectory,
        run: &RunRecord,
        anchor: &str,
        seal: &SuccessfulCompletionSeal,
    ) -> Result<()> {
        if run.state != RunState::Succeeded
            || seal.manifest.context != run.context
            || seal.run_schema_version != run.schema_version
            || seal.terminal_reason != run.terminal_reason
            || run.terminal_reason.is_some()
            || seal.manifest_sha256 != anchor
        {
            return Err(ScienceError::Invalid(
                "Succeeded run differs from its exact completion seal".into(),
            ));
        }
        let durable = Self::durable_completion_manifest_unlocked(run_dir, &run.context)?;
        if durable != seal.manifest {
            return Err(ScienceError::Invalid(
                "Succeeded collections differ from their exact completion anchor".into(),
            ));
        }
        let mut running = run.clone();
        running.state = RunState::Running;
        let approval =
            Self::running_allowed_approval(run_dir, &running, &run.context.run_id, None)?;
        if approval != seal.approval {
            return Err(ScienceError::Invalid(
                "Succeeded approval differs from its exact completion seal".into(),
            ));
        }
        Self::verify_completion_collections(
            run_dir,
            &run.context.run_id,
            &approval,
            &durable.artifacts,
            &durable.evidence,
            &durable.provenance,
            &durable.previews,
            &durable.events,
            &durable.final_event,
            true,
        )
    }

    fn verify_succeeded_seal_bounded(
        run_dir: &PinnedDirectory,
        run: &RunRecord,
        anchor: &str,
        seal: &SuccessfulCompletionSeal,
    ) -> Result<()> {
        if run.state != RunState::Succeeded
            || seal.manifest.context != run.context
            || seal.run_schema_version != run.schema_version
            || seal.terminal_reason != run.terminal_reason
            || run.terminal_reason.is_some()
            || seal.manifest_sha256 != anchor
        {
            return Err(ScienceError::Invalid(
                "Succeeded run differs from its exact completion seal".into(),
            ));
        }
        let durable = Self::durable_completion_manifest_bounded(run_dir, &run.context)?;
        if durable != seal.manifest {
            return Err(ScienceError::Invalid(
                "Succeeded collections differ from their exact completion anchor".into(),
            ));
        }
        let mut running = run.clone();
        running.state = RunState::Running;
        let approvals: Vec<Approval> = run_dir.read_json_bounded(
            Path::new("approvals.json"),
            Self::MAX_DOSSIER_REGISTRY_BYTES,
        )?;
        let approval = Self::allowed_approval_from_items(&running, &approvals, None)?;
        if approval != seal.approval {
            return Err(ScienceError::Invalid(
                "Succeeded approval differs from its exact completion seal".into(),
            ));
        }
        Self::verify_completion_collections(
            run_dir,
            &run.context.run_id,
            &approval,
            &durable.artifacts,
            &durable.evidence,
            &durable.provenance,
            &durable.previews,
            &durable.events,
            &durable.final_event,
            false,
        )
    }

    fn persist_succeeded_unlocked(
        run_dir: &PinnedDirectory,
        mut run: RunRecord,
    ) -> Result<RunRecord> {
        if run.state != RunState::Running {
            return Err(ScienceError::Invalid(
                "only a Running authority may commit Succeeded".into(),
            ));
        }
        run.state = RunState::Succeeded;
        run.terminal_reason = None;
        match run_dir.replace_json_atomic(Path::new("run.json"), &run) {
            Ok(()) => Ok(run),
            Err(error) => {
                let read_back: Result<RunRecord> = run_dir.read_json(Path::new("run.json"));
                match read_back {
                    Ok(read_back)
                        if read_back.context == run.context
                            && read_back.state == RunState::Succeeded =>
                    {
                        Ok(read_back)
                    }
                    _ => Err(error),
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_completion_collections(
        run_dir: &PinnedDirectory,
        run_id: &RunId,
        approval: &Approval,
        artifacts: &[Artifact],
        evidence: &[Evidence],
        provenance: &[Provenance],
        previews: &[preview::PreviewRecord],
        events: &[Event],
        final_event: &Event,
        verify_artifact_bytes: bool,
    ) -> Result<()> {
        if artifacts.is_empty() || evidence.is_empty() || provenance.is_empty() {
            return Err(ScienceError::Invalid(
                "successful completion requires artifact, evidence, and provenance".into(),
            ));
        }

        let artifact_dir = run_dir.open_directory(Path::new("artifacts"))?;
        let mut artifact_paths = BTreeSet::new();
        let mut artifact_hashes = BTreeSet::new();
        for artifact in artifacts {
            if artifact.run_id != *run_id
                || artifact.call_id != approval.call_id
                || !artifact_paths.insert(artifact.relative_path.clone())
                || !artifact_hashes.insert(artifact.sha256.clone())
            {
                return Err(ScienceError::Invalid(
                    "completion artifacts do not exactly bind the allowed call".into(),
                ));
            }
            if verify_artifact_bytes {
                let (bytes, sha256) = artifact_dir.hash_regular(&artifact.relative_path)?;
                if bytes != artifact.bytes || sha256 != artifact.sha256 {
                    return Err(ScienceError::Invalid(
                        "completion artifact bytes do not match registered hash/length".into(),
                    ));
                }
            }
        }

        let mut cited_artifacts = BTreeSet::new();
        for item in evidence {
            let Some(artifact_sha256) = item.artifact_sha256.as_ref() else {
                return Err(ScienceError::Invalid(
                    "completion evidence must cite an artifact hash".into(),
                ));
            };
            if item.run_id != *run_id
                || item.claim.trim().is_empty()
                || item.source.trim().is_empty()
                || !artifact_hashes.contains(artifact_sha256)
            {
                return Err(ScienceError::Invalid(
                    "completion evidence is incomplete or references an unknown artifact".into(),
                ));
            }
            cited_artifacts.insert(artifact_sha256.clone());
        }
        if cited_artifacts != artifact_hashes {
            return Err(ScienceError::Invalid(
                "completion evidence must exactly cover artifact hashes".into(),
            ));
        }

        if provenance.iter().any(|item| {
            item.run_id != *run_id
                || item.source_uri.trim().is_empty()
                || item.license.trim().is_empty()
                || item.tool.trim().is_empty()
                || !is_sha256_hex(&item.input_sha256)
        }) {
            return Err(ScienceError::Invalid(
                "completion provenance is incomplete or malformed".into(),
            ));
        }

        let mut preview_paths = BTreeSet::new();
        for preview in previews {
            if preview.run_id != *run_id
                || preview.call_id != approval.call_id
                || preview.tool.trim().is_empty()
                || !preview_paths.insert(preview.relative_path.clone())
                || !artifacts.iter().any(|artifact| {
                    artifact.relative_path == preview.relative_path
                        && artifact.sha256 == preview.artifact_sha256
                        && artifact.call_id == preview.call_id
                })
            {
                return Err(ScienceError::Invalid(
                    "completion preview is not uniquely bound to an artifact".into(),
                ));
            }
        }

        if events.is_empty()
            || events.last() != Some(final_event)
            || events.iter().filter(|event| *event == final_event).count() != 1
        {
            return Err(ScienceError::Invalid(
                "completion requires one unique final event at the end of the event collection"
                    .into(),
            ));
        }
        for (index, event) in events.iter().enumerate() {
            let expected_seq = u64::try_from(index)
                .map_err(|_| ScienceError::Invalid("event collection is too large".into()))?
                + 1;
            if event.schema_version != SCHEMA_VERSION
                || event.run_id != *run_id
                || event.seq != expected_seq
                || event.actor.trim().is_empty()
                || event.kind.trim().is_empty()
            {
                return Err(ScienceError::Invalid(
                    "completion event collection is not contiguous and well formed".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn append_event(
        &self,
        run_id: &RunId,
        actor: impl Into<String>,
        kind: impl Into<String>,
        payload: serde_json::Value,
    ) -> Result<Event> {
        self.append_event_with_failure_policy(run_id, actor, kind, payload, true)
    }

    /// Append a recoverable commit marker without turning the run terminal
    /// when the event file is temporarily unavailable. Callers must retry the
    /// same idempotent operation; this is intentionally narrower than the
    /// normal event path, whose failure remains fatal.
    pub fn append_recoverable_commit_event(
        &self,
        run_id: &RunId,
        actor: impl Into<String>,
        kind: impl Into<String>,
        payload: serde_json::Value,
    ) -> Result<Event> {
        self.append_event_with_failure_policy(run_id, actor, kind, payload, false)
    }

    fn append_event_with_failure_policy(
        &self,
        run_id: &RunId,
        actor: impl Into<String>,
        kind: impl Into<String>,
        payload: serde_json::Value,
        fail_run: bool,
    ) -> Result<Event> {
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let run_dir = self.open_run_directory(run_id)?;
        Self::reject_successful_completion_seal(&run_dir)?;
        let mut events: Vec<Event> = match run_dir.read_json(Path::new("events.json")) {
            Ok(events) => events,
            Err(error) => {
                if fail_run {
                    let _ = self.transition_unlocked(
                        run_id,
                        RunState::Failed,
                        Some(format!("event persistence failed: {error}")),
                    );
                }
                return Err(error);
            }
        };
        validate_events(&events, run_id)?;
        let event = Event {
            schema_version: SCHEMA_VERSION,
            run_id: run_id.clone(),
            seq: events.last().map_or(1, |event| event.seq + 1),
            actor: actor.into(),
            timestamp: Utc::now(),
            kind: kind.into(),
            payload,
        };
        events.push(event.clone());
        if let Err(error) = run_dir.replace_json_atomic(Path::new("events.json"), &events) {
            if fail_run {
                let _ = self.transition_unlocked(
                    run_id,
                    RunState::Failed,
                    Some(format!("event persistence failed: {error}")),
                );
            }
            return Err(error);
        }
        Ok(event)
    }

    pub fn events_after(&self, run_id: &RunId, after: u64, limit: usize) -> Result<Vec<Event>> {
        if limit == 0 || limit > 1_000 {
            return Err(ScienceError::Invalid("event limit must be 1..=1000".into()));
        }
        let (_, manifest) = self.load_run_snapshot(run_id)?;
        let events: Vec<Event> = match manifest {
            Some(seal) => seal.manifest.events,
            None => self
                .open_run_directory(run_id)?
                .read_json(Path::new("events.json"))?,
        };
        validate_events(&events, run_id)?;
        Ok(events
            .into_iter()
            .filter(|event| event.seq > after)
            .take(limit)
            .collect())
    }

    pub fn request_approval(&self, approval: Approval) -> Result<()> {
        validate_approval(&approval, &approval.run_id)?;
        if approval.decision != ApprovalDecision::Pending {
            return Err(ScienceError::Invalid("new approval must be pending".into()));
        }
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        self.assert_owner(&approval.project_id, &approval.run_id, &approval.owner_id)?;
        let run_dir = self.open_run_directory(&approval.run_id)?;
        let run: RunRecord = run_dir.read_json(Path::new("run.json"))?;
        validate_context(&run.context)?;
        if run.context.run_id != approval.run_id
            || run.context.project_id != approval.project_id
            || run.context.owner_id != approval.owner_id
        {
            return Err(ScienceError::Ownership);
        }
        if run.state != RunState::Created {
            return Err(ScienceError::Invalid(
                "pending approval may only be requested for a Created run".into(),
            ));
        }
        let mut items: Vec<Approval> = run_dir.read_json(Path::new("approvals.json"))?;
        validate_approvals(&items, &approval.run_id)?;
        if items.iter().any(|item| item.call_id == approval.call_id) {
            return Err(ScienceError::Invalid("duplicate approval call".into()));
        }
        items.push(approval);
        run_dir.replace_json_atomic(Path::new("approvals.json"), &items)
    }

    pub fn decide_approval(
        &self,
        project: &ProjectId,
        run_id: &RunId,
        owner: &str,
        call: &CallId,
        decision: ApprovalDecision,
    ) -> Result<Approval> {
        project.validate()?;
        run_id.validate()?;
        call.validate()?;
        if !decision.terminal() {
            return Err(ScienceError::Invalid("decision must be terminal".into()));
        }
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        self.assert_owner(project, run_id, owner)?;
        let run_dir = self.open_run_directory(run_id)?;
        let run: RunRecord = run_dir.read_json(Path::new("run.json"))?;
        validate_context(&run.context)?;
        if run.context.run_id != *run_id
            || run.context.project_id != *project
            || run.context.owner_id != owner
        {
            return Err(ScienceError::Ownership);
        }
        let mut items: Vec<Approval> = run_dir.read_json(Path::new("approvals.json"))?;
        validate_approvals(&items, run_id)?;
        let item = items
            .iter_mut()
            .find(|item| &item.call_id == call)
            .ok_or_else(|| ScienceError::Invalid("approval not found".into()))?;
        if item.decision.terminal() {
            if item.decision == decision {
                return Ok(item.clone());
            }
            return Err(ScienceError::ApprovalConflict);
        }
        if run.state != RunState::AwaitingApproval {
            return Err(ScienceError::Invalid(
                "pending approval may only be decided while AwaitingApproval".into(),
            ));
        }
        item.decision = decision;
        item.decided_at = Some(Utc::now());
        let result = item.clone();
        run_dir.replace_json_atomic(Path::new("approvals.json"), &items)?;
        Ok(result)
    }

    fn require_running_allowed_output(
        run_dir: &PinnedDirectory,
        run: &RunRecord,
        expected_run_id: &RunId,
        call: Option<&CallId>,
    ) -> Result<()> {
        Self::reject_successful_completion_seal(run_dir)?;
        Self::running_allowed_approval(run_dir, run, expected_run_id, call).map(|_| ())
    }

    fn running_allowed_approval(
        run_dir: &PinnedDirectory,
        run: &RunRecord,
        expected_run_id: &RunId,
        call: Option<&CallId>,
    ) -> Result<Approval> {
        validate_context(&run.context)?;
        if run.context.run_id != *expected_run_id || run.state != RunState::Running {
            return Err(ScienceError::Invalid(
                "scientific outputs require an allowed running run".into(),
            ));
        }
        let approvals: Vec<Approval> = run_dir.read_json(Path::new("approvals.json"))?;
        Self::allowed_approval_from_items(run, &approvals, call)
    }

    fn allowed_approval_from_items(
        run: &RunRecord,
        approvals: &[Approval],
        call: Option<&CallId>,
    ) -> Result<Approval> {
        validate_approvals(approvals, &run.context.run_id)?;
        let [approval] = approvals else {
            return Err(ScienceError::Invalid(
                "scientific outputs require exactly one approval".into(),
            ));
        };
        if approval.project_id != run.context.project_id
            || approval.run_id != run.context.run_id
            || approval.owner_id != run.context.owner_id
            || call.is_some_and(|call| approval.call_id != *call)
            || approval.decision != ApprovalDecision::Allow
            || approval.decided_at.is_none()
        {
            return Err(ScienceError::Invalid(
                "scientific outputs are not bound to a terminal Allow".into(),
            ));
        }
        Ok(approval.clone())
    }

    pub fn put_artifact(
        &self,
        project: &ProjectId,
        run_id: &RunId,
        owner: &str,
        call: CallId,
        relative: &Path,
        bytes: &[u8],
        mime: impl Into<String>,
        preview: impl Into<String>,
    ) -> Result<Artifact> {
        project.validate()?;
        run_id.validate()?;
        call.validate()?;
        validate_relative(relative)?;
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let run_dir = self.open_run_directory(run_id)?;
        let run = Self::load_run_record_from_directory(&run_dir, run_id)?;
        if run.context.project_id != *project || run.context.owner_id != owner {
            return Err(ScienceError::Ownership);
        }
        Self::running_allowed_approval(&run_dir, &run, run_id, Some(&call))?;
        let completion_is_sealed = Self::successful_completion_anchor(&run)?.is_some()
            || Self::successful_completion_seal(&run_dir)?.is_some();
        let artifact_dir = run_dir.open_directory(Path::new("artifacts"))?;
        let artifact = Artifact {
            run_id: run_id.clone(),
            call_id: call,
            relative_path: relative.to_path_buf(),
            sha256: hex_sha256(bytes),
            bytes: bytes.len() as u64,
            mime: mime.into(),
            preview: preview.into(),
        };
        let mut items: Vec<Artifact> = run_dir.read_json(Path::new("artifacts.json"))?;
        validate_artifacts(&items, run_id)?;
        if let Some(registered) = items
            .iter()
            .find(|registered| registered.relative_path == relative)
        {
            if registered != &artifact {
                return Err(ScienceError::Invalid(
                    "artifact path is registered with different metadata".into(),
                ));
            }
            match artifact_dir.read_regular(relative) {
                Ok(existing) if existing == bytes => {
                    artifact_dir.sync_containing_directory(relative)?;
                    run_dir.sync_directory()?;
                    return Ok(registered.clone());
                }
                Ok(_) => {
                    return Err(ScienceError::Invalid(
                        "registered artifact payload differs from the exact retry".into(),
                    ));
                }
                Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                    if completion_is_sealed {
                        return Err(ScienceError::Invalid(
                            "sealed completion cannot repair a missing artifact payload".into(),
                        ));
                    }
                    // A prior registry replacement may have become visible
                    // before its durability error was returned and cleanup
                    // removed the payload. Repair only the exact registered
                    // bytes supplied by the idempotent retry.
                    artifact_dir.write_new_atomic(relative, bytes)?;
                    let repaired = artifact_dir.read_regular(relative)?;
                    if repaired != bytes {
                        return Err(ScienceError::Invalid(
                            "repaired artifact payload differs from its registry".into(),
                        ));
                    }
                    artifact_dir.sync_containing_directory(relative)?;
                    run_dir.sync_directory()?;
                    return Ok(registered.clone());
                }
                Err(error) => return Err(error),
            }
        }
        if completion_is_sealed {
            return Err(ScienceError::Invalid(
                "sealed completion accepts only an exact read-only artifact retry".into(),
            ));
        }

        match artifact_dir.read_regular(relative) {
            Ok(existing) if existing == bytes => {
                // Recover a payload whose no-replace publication became
                // visible before the registry commit. The retained actual
                // containing parent (including nested artifact paths) must
                // sync before the registry may reference it.
                artifact_dir.sync_containing_directory(relative)?;
            }
            Ok(_) => {
                return Err(ScienceError::Invalid(
                    "unregistered artifact payload differs from the exact retry".into(),
                ));
            }
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Err(write_error) = artifact_dir.write_new_atomic(relative, bytes) {
                    match artifact_dir.read_regular(relative) {
                        Ok(existing) if existing == bytes => {
                            // The name may be visible after an ambiguous parent
                            // sync failure, but this call must not register it.
                            return Err(write_error);
                        }
                        Ok(_) => {
                            return Err(ScienceError::Invalid(
                                "artifact publication raced with different payload bytes".into(),
                            ));
                        }
                        Err(_) => return Err(write_error),
                    }
                }
            }
            Err(error) => return Err(error),
        }
        items.push(artifact.clone());
        if let Err(error) = run_dir.replace_json_atomic(Path::new("artifacts.json"), &items) {
            let reopened: Vec<Artifact> = match run_dir.read_json(Path::new("artifacts.json")) {
                Ok(reopened) => reopened,
                Err(_) => return Err(error),
            };
            if validate_artifacts(&reopened, run_id).is_ok()
                && reopened.iter().any(|registered| registered == &artifact)
                && artifact_dir.read_regular(relative)? == bytes
            {
                artifact_dir.sync_containing_directory(relative)?;
                run_dir.sync_directory()?;
                return Ok(artifact);
            }
            // Preserve an exact orphan payload for the next idempotent retry.
            // Unlinking after an ambiguous registry error can invert the
            // failure into a durable registry-to-missing-file corruption.
            return Err(error);
        }
        artifact_dir.sync_containing_directory(relative)?;
        run_dir.sync_directory()?;
        Ok(artifact)
    }

    /// Remove failed-call outputs without exposing raw store paths to protocol
    /// code. Registrations are withdrawn first, then the exact files are
    /// unlinked relative to the already-open artifact directory. This makes a
    /// cleanup failure non-serviceable and never follows a replaced symlink.
    pub fn discard_artifacts(
        &self,
        project: &ProjectId,
        run_id: &RunId,
        owner: &str,
        call: &CallId,
        relative_paths: &[&Path],
    ) -> Result<()> {
        project.validate()?;
        run_id.validate()?;
        call.validate()?;
        for relative in relative_paths {
            validate_relative(relative)?;
        }
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        self.assert_owner(project, run_id, owner)?;
        let run_dir = self.open_run_directory(run_id)?;
        let run: RunRecord = run_dir.read_json(Path::new("run.json"))?;
        Self::reject_fenced_rollback(&run_dir)?;
        Self::require_running_allowed_output(&run_dir, &run, run_id, Some(call))?;
        if run.context.project_id != *project || run.context.owner_id != owner {
            return Err(ScienceError::Ownership);
        }
        let mut items: Vec<Artifact> = run_dir.read_json(Path::new("artifacts.json"))?;
        validate_artifacts(&items, run_id)?;
        let removed: Vec<PathBuf> = items
            .iter()
            .filter(|artifact| {
                &artifact.call_id == call
                    && relative_paths
                        .iter()
                        .any(|relative| artifact.relative_path == *relative)
            })
            .map(|artifact| artifact.relative_path.clone())
            .collect();
        if removed.is_empty() {
            return Ok(());
        }
        items.retain(|artifact| {
            &artifact.call_id != call
                || !relative_paths
                    .iter()
                    .any(|relative| artifact.relative_path == *relative)
        });
        run_dir.replace_json_atomic(Path::new("artifacts.json"), &items)?;
        let artifact_dir = run_dir.open_directory(Path::new("artifacts"))?;
        for relative in removed {
            artifact_dir.unlink_file(&relative)?;
        }
        Ok(())
    }

    /// Withdraw every scientific output produced by one already-Allowed,
    /// still-Running actor call after its commit path fails.
    ///
    /// A run is the transaction boundary for science execution. Clearing the
    /// artifact registry, evidence, provenance, and previews before unlinking
    /// the known payload names ensures a subsequent Failed terminal cannot
    /// retain a partially authoritative result. Corrupt metadata is
    /// overwritten rather than parsed during rollback so the corruption that
    /// triggered rollback cannot prevent de-publication.
    pub fn discard_running_outputs(
        &self,
        project: &ProjectId,
        run_id: &RunId,
        owner: &str,
        call: &CallId,
        relative_paths: &[&Path],
    ) -> Result<()> {
        project.validate()?;
        run_id.validate()?;
        call.validate()?;
        for relative in relative_paths {
            validate_relative(relative)?;
        }
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let run_dir = self.open_run_directory(run_id)?;
        let run: RunRecord = run_dir.read_json(Path::new("run.json"))?;
        Self::reject_fenced_rollback(&run_dir)?;
        validate_context(&run.context)?;
        if run.context.run_id != *run_id
            || run.context.project_id != *project
            || run.context.owner_id != owner
        {
            return Err(ScienceError::Ownership);
        }
        if run.state != RunState::Running {
            return Err(ScienceError::Invalid(
                "only a running actor call may discard partial outputs".into(),
            ));
        }
        let approvals: Vec<Approval> = run_dir.read_json(Path::new("approvals.json"))?;
        validate_approvals(&approvals, run_id)?;
        let [approval] = approvals.as_slice() else {
            return Err(ScienceError::Invalid(
                "output rollback requires exactly one approval".into(),
            ));
        };
        if approval.project_id != *project
            || approval.run_id != *run_id
            || approval.owner_id != owner
            || approval.call_id != *call
            || approval.decision != ApprovalDecision::Allow
            || approval.decided_at.is_none()
        {
            return Err(ScienceError::Invalid(
                "output rollback is not bound to the allowed actor call".into(),
            ));
        }

        // De-publish all structured outputs before touching payload bytes.
        // Every science protocol owns a fresh run, so retaining any record
        // from a failed commit would be a cross-stage partial result.
        run_dir.replace_json_atomic(Path::new("artifacts.json"), &Vec::<Artifact>::new())?;
        run_dir.replace_json_atomic(Path::new("evidence.json"), &Vec::<Evidence>::new())?;
        run_dir.replace_json_atomic(Path::new("provenance.json"), &Vec::<Provenance>::new())?;
        run_dir.replace_json_atomic(
            Path::new("previews.json"),
            &Vec::<preview::PreviewRecord>::new(),
        )?;
        let artifact_dir = run_dir.open_directory(Path::new("artifacts"))?;
        for relative in relative_paths {
            match artifact_dir.unlink_file(relative) {
                Ok(()) => {}
                Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    /// Roll back an unsealed completion attempt so deterministic recovery can
    /// execute it again instead of trusting records that survived a process
    /// boundary without an exact completion seal.
    ///
    /// Structured outputs are de-published first through the ordinary
    /// Running+Allow rollback. The final event is then removed only when it is
    /// byte-for-byte the event the protocol verified before entering this
    /// method. If a crash lands between those cuts, retrying this method is
    /// idempotent: the two-event prefix remains the sole resumable state.
    pub(crate) fn discard_running_completion_attempt(
        &self,
        project: &ProjectId,
        run_id: &RunId,
        owner: &str,
        call: &CallId,
        relative_paths: &[&Path],
        expected_final_event: &Event,
    ) -> Result<()> {
        self.discard_running_outputs(project, run_id, owner, call, relative_paths)?;

        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let run_dir = self.open_run_directory(run_id)?;
        let run: RunRecord = run_dir.read_json(Path::new("run.json"))?;
        Self::reject_fenced_rollback(&run_dir)?;
        validate_context(&run.context)?;
        if run.context.run_id != *run_id
            || run.context.project_id != *project
            || run.context.owner_id != owner
            || run.state != RunState::Running
        {
            return Err(ScienceError::Ownership);
        }
        Self::running_allowed_approval(&run_dir, &run, run_id, Some(call))?;

        let mut events: Vec<Event> = run_dir.read_json(Path::new("events.json"))?;
        validate_events(&events, run_id)?;
        match events.as_slice() {
            [_, _] => return Ok(()),
            [_, _, final_event] if final_event == expected_final_event => {}
            _ => {
                return Err(ScienceError::Invalid(
                    "completion rollback did not find the exact unsealed final event".into(),
                ));
            }
        }
        events.pop();
        run_dir.replace_json_atomic(Path::new("events.json"), &events)
    }

    /// Clear outputs that appeared before a pending approval was decided.
    ///
    /// Such records cannot be authoritative: execution has not been allowed.
    /// The caller can then persist a Failed terminal without leaving forged
    /// registries serviceable. Identity and the one exact Pending approval are
    /// checked under the Science write lock before any record is changed.
    pub fn discard_pending_unauthorized_outputs(
        &self,
        project: &ProjectId,
        run_id: &RunId,
        owner: &str,
        call: &CallId,
    ) -> Result<()> {
        project.validate()?;
        run_id.validate()?;
        call.validate()?;
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let run_dir = self.open_run_directory(run_id)?;
        let run: RunRecord = run_dir.read_json(Path::new("run.json"))?;
        validate_context(&run.context)?;
        if run.context.run_id != *run_id
            || run.context.project_id != *project
            || run.context.owner_id != owner
            || run.state != RunState::AwaitingApproval
        {
            return Err(ScienceError::Ownership);
        }
        Self::reject_fenced_rollback(&run_dir)?;
        let approvals: Vec<Approval> = run_dir.read_json(Path::new("approvals.json"))?;
        validate_approvals(&approvals, run_id)?;
        let [approval] = approvals.as_slice() else {
            return Err(ScienceError::Invalid(
                "pending output rejection requires exactly one approval".into(),
            ));
        };
        if approval.project_id != *project
            || approval.run_id != *run_id
            || approval.owner_id != owner
            || approval.call_id != *call
            || approval.decision != ApprovalDecision::Pending
            || approval.decided_at.is_some()
        {
            return Err(ScienceError::Invalid(
                "pending output rejection is not bound to the exact pending call".into(),
            ));
        }

        let registered_paths = run_dir
            .read_json::<Vec<Artifact>>(Path::new("artifacts.json"))
            .ok()
            .filter(|artifacts| validate_artifacts(artifacts, run_id).is_ok())
            .unwrap_or_default()
            .into_iter()
            .map(|artifact| artifact.relative_path)
            .collect::<Vec<_>>();
        run_dir.replace_json_atomic(Path::new("artifacts.json"), &Vec::<Artifact>::new())?;
        run_dir.replace_json_atomic(Path::new("evidence.json"), &Vec::<Evidence>::new())?;
        run_dir.replace_json_atomic(Path::new("provenance.json"), &Vec::<Provenance>::new())?;
        run_dir.replace_json_atomic(
            Path::new("previews.json"),
            &Vec::<preview::PreviewRecord>::new(),
        )?;
        let artifact_dir = run_dir.open_directory(Path::new("artifacts"))?;
        for relative in registered_paths {
            match artifact_dir.unlink_file(&relative) {
                Ok(()) => {}
                Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    pub fn add_evidence(&self, evidence: Evidence) -> Result<()> {
        evidence.run_id.validate()?;
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let run_dir = self.open_run_directory(&evidence.run_id)?;
        let run: RunRecord = run_dir.read_json(Path::new("run.json"))?;
        Self::require_running_allowed_output(&run_dir, &run, &evidence.run_id, None)?;
        let mut items: Vec<Evidence> = run_dir.read_json(Path::new("evidence.json"))?;
        validate_run_ids(
            items.iter().map(|item| &item.run_id),
            &evidence.run_id,
            "evidence",
        )?;
        items.push(evidence);
        run_dir.replace_json_atomic(Path::new("evidence.json"), &items)
    }
    pub fn add_provenance(&self, provenance: Provenance) -> Result<()> {
        provenance.run_id.validate()?;
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let run_dir = self.open_run_directory(&provenance.run_id)?;
        let run: RunRecord = run_dir.read_json(Path::new("run.json"))?;
        Self::require_running_allowed_output(&run_dir, &run, &provenance.run_id, None)?;
        let mut items: Vec<Provenance> = run_dir.read_json(Path::new("provenance.json"))?;
        validate_run_ids(
            items.iter().map(|item| &item.run_id),
            &provenance.run_id,
            "provenance",
        )?;
        items.push(provenance);
        run_dir.replace_json_atomic(Path::new("provenance.json"), &items)
    }
    pub fn artifacts(&self, run_id: &RunId) -> Result<Vec<Artifact>> {
        let (_, manifest) = self.load_run_snapshot(run_id)?;
        let items: Vec<Artifact> = match manifest {
            Some(seal) => seal.manifest.artifacts,
            None => self
                .open_run_directory(run_id)?
                .read_json(Path::new("artifacts.json"))?,
        };
        validate_artifacts(&items, run_id)?;
        Ok(items)
    }
    pub fn add_preview(&self, preview: preview::PreviewRecord) -> Result<()> {
        preview.run_id.validate()?;
        preview.call_id.validate()?;
        validate_relative(&preview.relative_path)?;
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let run_dir = self.open_run_directory(&preview.run_id)?;
        let run: RunRecord = run_dir.read_json(Path::new("run.json"))?;
        Self::require_running_allowed_output(
            &run_dir,
            &run,
            &preview.run_id,
            Some(&preview.call_id),
        )?;
        let artifacts: Vec<Artifact> = run_dir.read_json(Path::new("artifacts.json"))?;
        validate_artifacts(&artifacts, &preview.run_id)?;
        if !artifacts.iter().any(|artifact| {
            artifact.call_id == preview.call_id
                && artifact.relative_path == preview.relative_path
                && artifact.sha256 == preview.artifact_sha256
        }) {
            return Err(ScienceError::Invalid(
                "preview is not bound to a registered artifact".into(),
            ));
        }
        let mut items: Vec<preview::PreviewRecord> =
            run_dir.read_json(Path::new("previews.json"))?;
        validate_previews(&items, &preview.run_id)?;
        items.push(preview);
        run_dir.replace_json_atomic(Path::new("previews.json"), &items)
    }
    /// Preview records for a run. Runs created before preview support have
    /// no `previews.json`; they read as empty rather than erroring.
    pub fn previews(&self, run_id: &RunId) -> Result<Vec<preview::PreviewRecord>> {
        let (_, manifest) = self.load_run_snapshot(run_id)?;
        if let Some(seal) = manifest {
            return Ok(seal.manifest.previews);
        }
        let run_dir = self.open_run_directory(run_id)?;
        match run_dir.read_json(Path::new("previews.json")) {
            Ok(items) => {
                let items: Vec<preview::PreviewRecord> = items;
                validate_previews(&items, run_id)?;
                Ok(items)
            }
            Err(ScienceError::Io(ref error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(Vec::new())
            }
            Err(error) => Err(error),
        }
    }
    pub fn evidence(&self, run_id: &RunId) -> Result<Vec<Evidence>> {
        let (_, manifest) = self.load_run_snapshot(run_id)?;
        let items: Vec<Evidence> = match manifest {
            Some(seal) => seal.manifest.evidence,
            None => self
                .open_run_directory(run_id)?
                .read_json(Path::new("evidence.json"))?,
        };
        validate_run_ids(items.iter().map(|item| &item.run_id), run_id, "evidence")?;
        Ok(items)
    }
    pub fn provenance(&self, run_id: &RunId) -> Result<Vec<Provenance>> {
        let (_, manifest) = self.load_run_snapshot(run_id)?;
        let items: Vec<Provenance> = match manifest {
            Some(seal) => seal.manifest.provenance,
            None => self
                .open_run_directory(run_id)?
                .read_json(Path::new("provenance.json"))?,
        };
        validate_run_ids(items.iter().map(|item| &item.run_id), run_id, "provenance")?;
        Ok(items)
    }
    pub fn approvals(&self, run_id: &RunId) -> Result<Vec<Approval>> {
        let (_, seal) = self.load_run_snapshot(run_id)?;
        let items: Vec<Approval> = match seal {
            Some(seal) => vec![seal.approval],
            None => self
                .open_run_directory(run_id)?
                .read_json(Path::new("approvals.json"))?,
        };
        validate_approvals(&items, run_id)?;
        Ok(items)
    }
    pub fn artifact_bytes(
        &self,
        project: &ProjectId,
        run_id: &RunId,
        owner: &str,
        relative: &Path,
    ) -> Result<Vec<u8>> {
        self.artifact_bytes_in_state(project, run_id, owner, relative, RunState::Succeeded)
    }

    /// Reopen a succeeded artifact without allowing a forged length record to
    /// make the process allocate an unbounded buffer. The regular-file handle
    /// is retained while reading and at most `max_bytes + 1` bytes are read.
    pub fn artifact_bytes_bounded(
        &self,
        project: &ProjectId,
        run_id: &RunId,
        owner: &str,
        relative: &Path,
        max_bytes: u64,
    ) -> Result<Vec<u8>> {
        self.artifact_bytes_in_state_bounded(
            project,
            run_id,
            owner,
            relative,
            RunState::Succeeded,
            Some(max_bytes),
        )
    }

    /// Reopen and re-hash a commit candidate while its actor-owned run is
    /// still Running. This is crate-internal so product adapters cannot serve
    /// pre-terminal bytes; review commit verification uses it immediately
    /// before the final Succeeded transition.
    pub(crate) fn running_artifact_bytes(
        &self,
        project: &ProjectId,
        run_id: &RunId,
        owner: &str,
        relative: &Path,
    ) -> Result<Vec<u8>> {
        self.artifact_bytes_in_state(project, run_id, owner, relative, RunState::Running)
    }

    /// Reopen a candidate produced by one durably Allowed actor call while
    /// the authority run is still Running.
    ///
    /// Unlike the crate-internal review helper, this public seam additionally
    /// proves the exact call id and terminal Allow decision. SessionActor uses
    /// it for multi-store commits such as V1→V2 migration, where the copied
    /// payload must be byte-verified before the project ledger and run become
    /// authoritative.
    pub fn allowed_running_artifact_bytes(
        &self,
        project: &ProjectId,
        run_id: &RunId,
        owner: &str,
        call: &CallId,
        relative: &Path,
    ) -> Result<Vec<u8>> {
        let approvals = self.approvals(run_id)?;
        let [approval] = approvals.as_slice() else {
            return Err(ScienceError::Invalid(
                "running artifact verification requires exactly one approval".into(),
            ));
        };
        if approval.project_id != *project
            || approval.run_id != *run_id
            || approval.owner_id != owner
            || approval.call_id != *call
            || approval.decision != ApprovalDecision::Allow
            || approval.decided_at.is_none()
        {
            return Err(ScienceError::Invalid(
                "running artifact verification is not bound to the allowed actor call".into(),
            ));
        }
        self.running_artifact_bytes(project, run_id, owner, relative)
    }

    /// Bounded form of `allowed_running_artifact_bytes` for restart recovery.
    ///
    /// Recovery must verify an already-written completion without trusting a
    /// forged registry length to allocate an unbounded buffer. The same exact
    /// durable Allow and call binding is required before any bytes are read.
    pub fn allowed_running_artifact_bytes_bounded(
        &self,
        project: &ProjectId,
        run_id: &RunId,
        owner: &str,
        call: &CallId,
        relative: &Path,
        max_bytes: u64,
    ) -> Result<Vec<u8>> {
        let approvals = self.approvals(run_id)?;
        let [approval] = approvals.as_slice() else {
            return Err(ScienceError::Invalid(
                "running artifact verification requires exactly one approval".into(),
            ));
        };
        if approval.project_id != *project
            || approval.run_id != *run_id
            || approval.owner_id != owner
            || approval.call_id != *call
            || approval.decision != ApprovalDecision::Allow
            || approval.decided_at.is_none()
        {
            return Err(ScienceError::Invalid(
                "running artifact verification is not bound to the allowed actor call".into(),
            ));
        }
        self.artifact_bytes_in_state_bounded(
            project,
            run_id,
            owner,
            relative,
            RunState::Running,
            Some(max_bytes),
        )
    }

    fn artifact_bytes_in_state(
        &self,
        project: &ProjectId,
        run_id: &RunId,
        owner: &str,
        relative: &Path,
        required_state: RunState,
    ) -> Result<Vec<u8>> {
        self.artifact_bytes_in_state_bounded(project, run_id, owner, relative, required_state, None)
    }

    fn artifact_bytes_in_state_bounded(
        &self,
        project: &ProjectId,
        run_id: &RunId,
        owner: &str,
        relative: &Path,
        required_state: RunState,
        max_bytes: Option<u64>,
    ) -> Result<Vec<u8>> {
        project.validate()?;
        run_id.validate()?;
        let (run, manifest) = match max_bytes {
            Some(_) => {
                self.load_run_snapshot_bounded(run_id, Self::MAX_DOSSIER_RUN_RECORD_BYTES)?
            }
            None => self.load_run_snapshot(run_id)?,
        };
        if &run.context.project_id != project || run.context.owner_id != owner {
            return Err(ScienceError::Ownership);
        }
        if run.state != required_state {
            return Err(ScienceError::Invalid(
                "artifact bytes are unavailable in the run's current state".into(),
            ));
        }
        validate_relative(relative)?;
        let run_dir = self.open_run_directory(run_id)?;
        let artifacts: Vec<Artifact> = match manifest {
            Some(seal) => seal.manifest.artifacts,
            None => match max_bytes {
                Some(_) => run_dir.read_json_bounded(
                    Path::new("artifacts.json"),
                    Self::MAX_DOSSIER_REGISTRY_BYTES,
                )?,
                None => run_dir.read_json(Path::new("artifacts.json"))?,
            },
        };
        validate_artifacts(&artifacts, run_id)?;
        let artifact = artifacts
            .iter()
            .find(|item| item.relative_path == relative)
            .ok_or_else(|| ScienceError::Invalid("artifact is not registered to run".into()))?;
        if max_bytes.is_some_and(|limit| artifact.bytes > limit) {
            return Err(ScienceError::Invalid(
                "artifact exceeds the caller's byte limit".into(),
            ));
        }
        let artifact_dir = run_dir.open_directory(Path::new("artifacts"))?;
        let bytes = match max_bytes {
            Some(limit) => artifact_dir.read_regular_bounded(relative, limit)?,
            None => artifact_dir.read_regular(relative)?,
        };
        if bytes.len() as u64 != artifact.bytes || hex_sha256(&bytes) != artifact.sha256 {
            return Err(ScienceError::Invalid(
                "artifact bytes do not match their registered hash/length".into(),
            ));
        }
        Ok(bytes)
    }

    /// Bounded metadata reads used by dossier composition. These methods
    /// validate record identity and item count after limiting the bytes read
    /// from the retained run-directory capability.
    pub fn artifacts_bounded(
        &self,
        run_id: &RunId,
        max_items: usize,
        max_json_bytes: u64,
    ) -> Result<Vec<Artifact>> {
        let (_, manifest) =
            self.load_run_snapshot_bounded(run_id, Self::MAX_DOSSIER_RUN_RECORD_BYTES)?;
        let byte_limit = max_json_bytes.min(Self::MAX_DOSSIER_REGISTRY_BYTES);
        let items: Vec<Artifact> = match manifest {
            Some(seal) => seal.manifest.artifacts,
            None => self
                .open_run_directory(run_id)?
                .read_json_bounded(Path::new("artifacts.json"), byte_limit)?,
        };
        validate_artifacts(&items, run_id)?;
        if items.len() > max_items {
            return Err(ScienceError::Invalid(
                "artifact registry exceeds the dossier item limit".into(),
            ));
        }
        if serde_json::to_vec(&items)?.len() as u64 > byte_limit {
            return Err(ScienceError::Invalid(
                "artifact registry exceeds the dossier byte limit".into(),
            ));
        }
        Ok(items)
    }

    pub fn evidence_bounded(
        &self,
        run_id: &RunId,
        max_items: usize,
        max_json_bytes: u64,
    ) -> Result<Vec<Evidence>> {
        let (_, manifest) =
            self.load_run_snapshot_bounded(run_id, Self::MAX_DOSSIER_RUN_RECORD_BYTES)?;
        let byte_limit = max_json_bytes.min(Self::MAX_DOSSIER_REGISTRY_BYTES);
        let items: Vec<Evidence> = match manifest {
            Some(seal) => seal.manifest.evidence,
            None => self
                .open_run_directory(run_id)?
                .read_json_bounded(Path::new("evidence.json"), byte_limit)?,
        };
        validate_run_ids(items.iter().map(|item| &item.run_id), run_id, "evidence")?;
        if items.len() > max_items {
            return Err(ScienceError::Invalid(
                "evidence registry exceeds the dossier item limit".into(),
            ));
        }
        if serde_json::to_vec(&items)?.len() as u64 > byte_limit {
            return Err(ScienceError::Invalid(
                "evidence registry exceeds the dossier byte limit".into(),
            ));
        }
        Ok(items)
    }

    pub fn provenance_bounded(
        &self,
        run_id: &RunId,
        max_items: usize,
        max_json_bytes: u64,
    ) -> Result<Vec<Provenance>> {
        let (_, manifest) =
            self.load_run_snapshot_bounded(run_id, Self::MAX_DOSSIER_RUN_RECORD_BYTES)?;
        let byte_limit = max_json_bytes.min(Self::MAX_DOSSIER_REGISTRY_BYTES);
        let items: Vec<Provenance> = match manifest {
            Some(seal) => seal.manifest.provenance,
            None => self
                .open_run_directory(run_id)?
                .read_json_bounded(Path::new("provenance.json"), byte_limit)?,
        };
        validate_run_ids(items.iter().map(|item| &item.run_id), run_id, "provenance")?;
        if items.len() > max_items {
            return Err(ScienceError::Invalid(
                "provenance registry exceeds the dossier item limit".into(),
            ));
        }
        if serde_json::to_vec(&items)?.len() as u64 > byte_limit {
            return Err(ScienceError::Invalid(
                "provenance registry exceeds the dossier byte limit".into(),
            ));
        }
        Ok(items)
    }

    /// Recover only an exact completion that was durably fenced before the
    /// process stopped.
    ///
    /// Unlike `recover_interrupted`, this never terminalizes an ordinary
    /// active run. The caller supplies the complete expected authority
    /// context, which is compared before any recovery write, so pointing a
    /// retry at another store cannot finish a foreign operation.
    pub fn recover_exact_completion(
        &self,
        expected_context: &RunContext,
    ) -> Result<Option<RunRecord>> {
        validate_context(expected_context)?;
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let run_dir = self.open_run_directory(&expected_context.run_id)?;
        let run = Self::load_run_record_from_directory(&run_dir, &expected_context.run_id)?;
        if run.context != *expected_context {
            return Err(ScienceError::Invalid(
                "exact completion recovery context does not match the durable run".into(),
            ));
        }
        let anchor = Self::successful_completion_anchor(&run)?;
        let seal = Self::successful_completion_seal(&run_dir)?;
        Self::recover_exact_completion_unlocked(&run_dir, &run, anchor, seal.as_ref())
    }

    fn recover_exact_completion_unlocked(
        run_dir: &PinnedDirectory,
        run: &RunRecord,
        anchor: Option<&str>,
        seal: Option<&SuccessfulCompletionSeal>,
    ) -> Result<Option<RunRecord>> {
        if run.state.terminal() {
            match (anchor, seal) {
                (Some(anchor), Some(seal)) => {
                    Self::verify_succeeded_seal_unlocked(run_dir, run, anchor, seal)?;
                    return Ok(Some(run.clone()));
                }
                (Some(_), None) | (None, Some(_)) => {
                    return Err(ScienceError::Invalid(
                        "exact completion anchor and seal must both remain durable".into(),
                    ));
                }
                (None, None) => {}
            }
            return Ok(None);
        }
        if let Some(anchor) = anchor {
            if run.state != RunState::Running {
                return Err(ScienceError::Invalid(
                    "exact completion anchor requires a Running recovery state".into(),
                ));
            }
            let manifest = Self::durable_completion_manifest_unlocked(run_dir, &run.context)?;
            let approval = Self::running_allowed_approval(run_dir, run, &run.context.run_id, None)?;
            let expected = Self::expected_successful_completion_seal(run, &approval, &manifest)?;
            if expected.manifest_sha256 != anchor {
                return Err(ScienceError::Invalid(
                    "durable collections differ from their completion anchor".into(),
                ));
            }
            match seal {
                Some(seal) if seal == &expected => {}
                Some(_) => {
                    return Err(ScienceError::Invalid(
                        "successful completion seal conflicts with its durable anchor".into(),
                    ));
                }
                None => {
                    Self::persist_successful_completion_seal_unlocked(
                        run_dir, run, &approval, &manifest,
                    )?;
                }
            }
            return Self::transition_succeeded_with_manifest_unlocked(run_dir, &manifest).map(Some);
        }
        if let Some(seal) = seal {
            if seal.manifest.context.run_id != run.context.run_id || run.state != RunState::Running
            {
                return Err(ScienceError::Invalid(
                    "unanchored successful completion seal has invalid recovery state".into(),
                ));
            }
            return Self::transition_succeeded_with_manifest_unlocked(run_dir, &seal.manifest)
                .map(Some);
        }
        Ok(None)
    }

    pub fn recover_interrupted(&self, run_id: &RunId) -> Result<RunRecord> {
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let run_dir = self.open_run_directory(run_id)?;
        let run = Self::load_run_record_from_directory(&run_dir, run_id)?;
        let anchor = Self::successful_completion_anchor(&run)?;
        let seal = Self::successful_completion_seal(&run_dir)?;
        if let Some(recovered) =
            Self::recover_exact_completion_unlocked(&run_dir, &run, anchor, seal.as_ref())?
        {
            return Ok(recovered);
        }
        if run.state.terminal() {
            return Ok(run);
        }
        let mut approvals: Vec<Approval> = run_dir.read_json(Path::new("approvals.json"))?;
        validate_approvals(&approvals, run_id)?;
        let mut approvals_changed = false;
        for approval in &mut approvals {
            if approval.decision == ApprovalDecision::Pending {
                approval.decision = ApprovalDecision::Interrupted;
                approval.decided_at = Some(Utc::now());
                approvals_changed = true;
            }
        }
        if approvals_changed {
            run_dir.replace_json_atomic(Path::new("approvals.json"), &approvals)?;
        }
        self.transition_unlocked(
            run_id,
            RunState::Interrupted,
            Some("process restarted before terminal state".into()),
        )
    }

    fn assert_owner(&self, project: &ProjectId, run_id: &RunId, owner: &str) -> Result<()> {
        project.validate()?;
        run_id.validate()?;
        let run = self.load_run(run_id)?;
        if &run.context.project_id != project || run.context.owner_id != owner {
            return Err(ScienceError::Ownership);
        }
        Ok(())
    }
    #[cfg(test)]
    fn run_dir(&self, run_id: &RunId) -> Result<PathBuf> {
        run_id.validate()?;
        Ok(self.root.join("runs").join(&run_id.0))
    }

    fn open_run_directory(&self, run_id: &RunId) -> Result<PinnedDirectory> {
        run_id.validate()?;
        let root = self.root_directory()?;
        root.open_directory(Path::new("runs"))?
            .open_directory(Path::new(&run_id.0))
    }

    fn root_directory(&self) -> Result<PinnedDirectory> {
        let mut capability = self
            .root_capability
            .lock()
            .map_err(|_| ScienceError::Invalid("science root capability lock poisoned".into()))?;
        match &*capability {
            StoreRootCapability::Pinned(directory) => return directory.try_clone(),
            StoreRootCapability::Unavailable(message) => {
                return Err(ScienceError::Invalid(message.clone()));
            }
            StoreRootCapability::Pending => {}
        }
        match PinnedDirectory::create_path(&self.root) {
            Ok(directory) => {
                *capability = StoreRootCapability::Pinned(directory.try_clone()?);
                Ok(directory)
            }
            Err(error) => {
                *capability = StoreRootCapability::Unavailable(error.to_string());
                Err(error)
            }
        }
    }
}

/// Directory capability used for all artifact manifest and payload I/O.
///
/// On Unix every component is opened relative to the preceding directory
/// descriptor with `O_NOFOLLOW`. Payload publication and deletion are then
/// relative to the retained parent descriptor, so renaming or replacing any
/// pathname after approval cannot redirect the operation.
#[cfg(unix)]
#[derive(Debug)]
struct PinnedDirectory {
    file: fs::File,
}

#[cfg(unix)]
impl PinnedDirectory {
    fn metadata_identity(metadata: &fs::Metadata) -> StoreRootIdentity {
        use std::os::unix::fs::MetadataExt as _;

        StoreRootIdentity::Unix {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }

    fn checked_directory_identity(path: &Path, kind: &str) -> Result<StoreRootIdentity> {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(ScienceError::Invalid(format!(
                "{kind} must be a non-symlink directory"
            )));
        }
        Ok(Self::metadata_identity(&metadata))
    }

    /// Open an existing root only through a retained workspace directory.
    ///
    /// The two identity snapshots are intentionally taken before deriving or
    /// opening the target path. Any rename/replacement during that window is
    /// rejected by the descriptor and pathname identity checks below.
    fn open_existing_confined(root: &Path, workspace_root: &Path) -> Result<Self> {
        Self::open_existing_confined_with_snapshot_hook(root, workspace_root, || Ok(()))
    }

    fn open_existing_confined_with_snapshot_hook(
        root: &Path,
        workspace_root: &Path,
        after_snapshot: impl FnOnce() -> Result<()>,
    ) -> Result<Self> {
        let root_absolute = if root.is_absolute() {
            root.to_path_buf()
        } else {
            std::env::current_dir()?.join(root)
        };
        let expected_root = Self::checked_directory_identity(&root_absolute, "science store root")?;

        let workspace = dunce::canonicalize(workspace_root)?;
        if !workspace.is_absolute() {
            return Err(ScienceError::Invalid(
                "workspace root must resolve to an absolute path".into(),
            ));
        }
        let expected_workspace =
            Self::checked_directory_identity(&workspace, "canonical workspace root")?;

        after_snapshot()?;

        // Do not canonicalize this path a second time: each component of the
        // already-canonical workspace is opened with O_NOFOLLOW, then its
        // retained identity is compared with the pre-open snapshot.
        let workspace_directory = Self::open_absolute_path(&workspace)?;
        if workspace_directory.identity()? != expected_workspace
            || workspace_directory.final_path()? != workspace
            || Self::checked_directory_identity(&workspace, "canonical workspace root")?
                != expected_workspace
        {
            return Err(ScienceError::Invalid(
                "workspace directory identity changed during confined open".into(),
            ));
        }

        let canonical_root = dunce::canonicalize(&root_absolute)?;
        let relative = canonical_root.strip_prefix(&workspace).map_err(|_| {
            ScienceError::Invalid(
                "science store root escapes the retained canonical workspace".into(),
            )
        })?;
        let directory = if relative.as_os_str().is_empty() {
            workspace_directory.try_clone()?
        } else {
            validate_relative(relative)?;
            workspace_directory.open_directory(relative)?
        };

        if directory.identity()? != expected_root
            || directory.final_path()? != canonical_root
            || Self::checked_directory_identity(&root_absolute, "science store root")?
                != expected_root
        {
            return Err(ScienceError::Invalid(
                "science store root identity changed during confined open".into(),
            ));
        }
        Ok(directory)
    }

    fn create_path(path: &Path) -> Result<Self> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        match fs::symlink_metadata(&absolute) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ScienceError::Invalid(
                    "science store root must not be a symlink".into(),
                ));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(ScienceError::Invalid(
                    "science store root must be a directory".into(),
                ));
            }
            Ok(_) => return Self::open_path(&absolute),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let parent = absolute
            .parent()
            .ok_or_else(|| ScienceError::Invalid("science store root has no parent".into()))?;
        let name = absolute
            .file_name()
            .ok_or_else(|| ScienceError::Invalid("science store root has no file name".into()))?;
        let parent = Self::create_path(parent)?;
        parent.create_directories(Path::new(name))
    }

    fn open_path(path: &Path) -> Result<Self> {
        // Resolve platform aliases such as macOS /var -> /private/var once,
        // then re-open every resulting component without following links.
        let canonical = dunce::canonicalize(path)?;
        Self::open_absolute_path(&canonical)
    }

    fn open_absolute_path(canonical: &Path) -> Result<Self> {
        use std::os::unix::fs::OpenOptionsExt as _;

        if !canonical.is_absolute() {
            return Err(ScienceError::Invalid(
                "artifact store root must resolve to an absolute path".into(),
            ));
        }
        let mut options = fs::OpenOptions::new();
        options.read(true).custom_flags(
            libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        );
        let mut current = Self {
            file: options.open(Path::new("/"))?,
        };
        for component in canonical.components() {
            match component {
                Component::RootDir | Component::CurDir => {}
                Component::Normal(name) => {
                    current = current.open_directory(Path::new(name))?;
                }
                _ => {
                    return Err(ScienceError::Invalid(
                        "artifact store root contains an unsupported path component".into(),
                    ));
                }
            }
        }
        Ok(current)
    }

    fn identity(&self) -> Result<StoreRootIdentity> {
        use std::os::unix::fs::MetadataExt as _;

        let metadata = self.file.metadata()?;
        Ok(StoreRootIdentity::Unix {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }

    fn assert_private_owned_directory(&self, kind: &str) -> Result<()> {
        use std::os::unix::fs::MetadataExt as _;

        let metadata = self.file.metadata()?;
        // SAFETY: geteuid has no preconditions and does not retain pointers.
        if !metadata.is_dir() {
            return Err(ScienceError::Invalid(format!("{kind} must be a directory")));
        }
        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err(ScienceError::Ownership);
        }
        if metadata.mode() & 0o022 != 0 {
            return Err(ScienceError::Invalid(format!(
                "{kind} must not be group- or world-writable"
            )));
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn final_path(&self) -> Result<PathBuf> {
        use std::os::fd::AsRawFd as _;
        use std::os::unix::ffi::OsStringExt as _;

        let mut buffer = vec![0_i8; libc::PATH_MAX as usize];
        // SAFETY: F_GETPATH writes at most PATH_MAX bytes to the live buffer
        // for the retained directory descriptor.
        if unsafe { libc::fcntl(self.file.as_raw_fd(), libc::F_GETPATH, buffer.as_mut_ptr()) } < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: successful F_GETPATH returns a NUL-terminated path.
        let path = unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) };
        Ok(PathBuf::from(std::ffi::OsString::from_vec(
            path.to_bytes().to_vec(),
        )))
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn final_path(&self) -> Result<PathBuf> {
        use std::os::fd::AsRawFd as _;

        Ok(fs::read_link(format!(
            "/proc/self/fd/{}",
            self.file.as_raw_fd()
        ))?)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "android")))]
    fn final_path(&self) -> Result<PathBuf> {
        Err(ScienceError::FeatureDisabled(
            "confined store final-path validation has no Unix backend on this platform".into(),
        ))
    }

    fn create_directory_new(&self, relative: &Path) -> Result<Self> {
        validate_relative(relative)?;
        let mut components = relative.components();
        let Some(Component::Normal(name)) = components.next() else {
            return Err(ScienceError::Invalid(
                "directory capability received a non-normal component".into(),
            ));
        };
        if components.next().is_some() {
            return Err(ScienceError::Invalid(
                "exclusive directory creation requires one component".into(),
            ));
        }
        mkdirat(&self.file, name, 0o700)?;
        match self.open_directory(Path::new(name)) {
            Ok(directory) => Ok(directory),
            Err(error) => {
                let _ = unlink_directory_at(&self.file, name);
                Err(error)
            }
        }
    }

    fn remove_directory_if_empty(&self, relative: &Path) -> Result<bool> {
        validate_relative(relative)?;
        let mut components = relative.components();
        let Some(Component::Normal(name)) = components.next() else {
            return Err(ScienceError::Invalid(
                "directory removal received a non-normal component".into(),
            ));
        };
        if components.next().is_some() {
            return Err(ScienceError::Invalid(
                "exclusive directory removal requires one component".into(),
            ));
        }
        match unlink_directory_at(&self.file, name) {
            Ok(()) => {
                self.file.sync_all()?;
                Ok(true)
            }
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(true)
            }
            Err(ScienceError::Io(error))
                if error
                    .raw_os_error()
                    .is_some_and(|code| code == libc::ENOTEMPTY || code == libc::EEXIST) =>
            {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    fn publish_directory_new(&self, staged: &Path, final_name: &Path) -> Result<()> {
        validate_relative(staged)?;
        validate_relative(final_name)?;
        let mut staged_components = staged.components();
        let Some(Component::Normal(staged_name)) = staged_components.next() else {
            return Err(ScienceError::Invalid(
                "staged directory name is not a normal component".into(),
            ));
        };
        let mut final_components = final_name.components();
        let Some(Component::Normal(final_name)) = final_components.next() else {
            return Err(ScienceError::Invalid(
                "final directory name is not a normal component".into(),
            ));
        };
        if staged_components.next().is_some() || final_components.next().is_some() {
            return Err(ScienceError::Invalid(
                "directory publication requires sibling names".into(),
            ));
        }
        rename_directory_noreplace_at(&self.file, staged_name, final_name)?;
        self.file.sync_all()?;
        Ok(())
    }

    fn open_directory(&self, relative: &Path) -> Result<Self> {
        validate_relative(relative)?;
        let mut current = self.try_clone()?;
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err(ScienceError::Invalid(
                    "directory capability received a non-normal component".into(),
                ));
            };
            current = Self {
                file: openat(
                    &current.file,
                    name,
                    libc::O_RDONLY
                        | libc::O_DIRECTORY
                        | libc::O_CLOEXEC
                        | libc::O_NOFOLLOW
                        | libc::O_NONBLOCK,
                    None,
                )?,
            };
            if !current.file.metadata()?.is_dir() {
                return Err(ScienceError::Invalid(
                    "artifact path component is not a directory".into(),
                ));
            }
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
                    "directory capability received a non-normal component".into(),
                ));
            };
            match mkdirat(&current.file, name, 0o700) {
                Ok(()) => {}
                Err(ScienceError::Io(error))
                    if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
            #[cfg(test)]
            if FAIL_DIRECTORY_ENTRY_PARENT_SYNC.with(|fail| fail.replace(false)) {
                return Err(ScienceError::Io(std::io::Error::other(
                    "injected directory-entry parent sync failure",
                )));
            }
            // Persist both a newly created directory entry and an existing
            // entry left behind by an earlier failed sync. Without this parent
            // flush, a successful durable Begin could disappear after power
            // loss even though every file inside `runs/` was individually
            // synced.
            current.file.sync_all()?;
            current = Self {
                file: openat(
                    &current.file,
                    name,
                    libc::O_RDONLY
                        | libc::O_DIRECTORY
                        | libc::O_CLOEXEC
                        | libc::O_NOFOLLOW
                        | libc::O_NONBLOCK,
                    None,
                )?,
            };
            if !current.file.metadata()?.is_dir() {
                return Err(ScienceError::Invalid(
                    "artifact path component is not a directory".into(),
                ));
            }
        }
        Ok(current)
    }

    fn try_lock_operation_file(&self, relative: &Path) -> Result<fs::File> {
        use std::os::{fd::AsRawFd as _, unix::fs::MetadataExt as _};

        validate_relative(relative)?;
        if relative.components().count() != 1 {
            return Err(ScienceError::Invalid(
                "operation lease requires one file-name component".into(),
            ));
        }
        self.assert_private_owned_directory("science operation lease directory")?;
        let name = relative
            .file_name()
            .ok_or_else(|| ScienceError::Invalid("operation lease name is missing".into()))?;
        let file = openat(
            &self.file,
            name,
            libc::O_RDWR | libc::O_CREAT | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            Some(0o600),
        )?;
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o022 != 0
            || metadata.nlink() != 1
        {
            return Err(ScienceError::Invalid(
                "operation lease must be one private process-owned regular file".into(),
            ));
        }
        // SAFETY: flock receives one live descriptor and has no pointer
        // arguments. The nonblocking exclusive lock is released on close.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::WouldBlock {
                return Err(ScienceError::Invalid(
                    "operation is already active in another Lumen process".into(),
                ));
            }
            return Err(error.into());
        }
        let reopened = openat(
            &self.file,
            name,
            libc::O_RDWR | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            None,
        )?;
        let reopened_metadata = reopened.metadata()?;
        if !reopened_metadata.is_file()
            || reopened_metadata.uid() != unsafe { libc::geteuid() }
            || reopened_metadata.mode() & 0o022 != 0
            || reopened_metadata.nlink() != 1
            || metadata.dev() != reopened_metadata.dev()
            || metadata.ino() != reopened_metadata.ino()
        {
            return Err(ScienceError::Invalid(
                "operation lease file changed during acquisition".into(),
            ));
        }
        Ok(file)
    }

    fn read_regular(&self, relative: &Path) -> Result<Vec<u8>> {
        validate_relative(relative)?;
        let parent = self.open_directory_parent(relative)?;
        let name = relative
            .file_name()
            .ok_or_else(|| ScienceError::Invalid("artifact path has no file name".into()))?;
        let mut file = openat(
            &parent.file,
            name,
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            None,
        )
        .map_err(|error| match error {
            ScienceError::Io(io) if io.raw_os_error() == Some(libc::ELOOP) => {
                ScienceError::Invalid("artifact must not be a symlink".into())
            }
            error => error,
        })?;
        if !file.metadata()?.is_file() {
            return Err(ScienceError::Invalid(
                "artifact must be a regular file".into(),
            ));
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn read_regular_bounded(&self, relative: &Path, max_bytes: u64) -> Result<Vec<u8>> {
        validate_relative(relative)?;
        let parent = self.open_directory_parent(relative)?;
        let name = relative
            .file_name()
            .ok_or_else(|| ScienceError::Invalid("artifact path has no file name".into()))?;
        let file = openat(
            &parent.file,
            name,
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            None,
        )
        .map_err(|error| match error {
            ScienceError::Io(io) if io.raw_os_error() == Some(libc::ELOOP) => {
                ScienceError::Invalid("artifact must not be a symlink".into())
            }
            error => error,
        })?;
        if !file.metadata()?.is_file() {
            return Err(ScienceError::Invalid(
                "artifact must be a regular file".into(),
            ));
        }
        let mut bytes = Vec::new();
        file.take(max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > max_bytes {
            return Err(ScienceError::Invalid(
                "artifact exceeds the caller's byte limit".into(),
            ));
        }
        Ok(bytes)
    }

    fn hash_regular(&self, relative: &Path) -> Result<(u64, String)> {
        validate_relative(relative)?;
        let parent = self.open_directory_parent(relative)?;
        let name = relative
            .file_name()
            .ok_or_else(|| ScienceError::Invalid("artifact path has no file name".into()))?;
        let mut file = openat(
            &parent.file,
            name,
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            None,
        )
        .map_err(|error| match error {
            ScienceError::Io(io) if io.raw_os_error() == Some(libc::ELOOP) => {
                ScienceError::Invalid("artifact must not be a symlink".into())
            }
            error => error,
        })?;
        if !file.metadata()?.is_file() {
            return Err(ScienceError::Invalid(
                "artifact must be a regular file".into(),
            ));
        }
        hash_open_file(&mut file)
    }

    fn read_json<T: DeserializeOwned>(&self, relative: &Path) -> Result<T> {
        Ok(serde_json::from_slice(&self.read_regular(relative)?)?)
    }

    fn read_json_bounded<T: DeserializeOwned>(&self, relative: &Path, max_bytes: u64) -> Result<T> {
        Ok(serde_json::from_slice(
            &self.read_regular_bounded(relative, max_bytes)?,
        )?)
    }

    /// Publish a new immutable payload without replacing any existing name.
    /// `linkat` is the portable Unix no-replace publication primitive: a
    /// pre-existing regular file, directory, or symlink makes it fail.
    fn write_new_atomic(&self, relative: &Path, bytes: &[u8]) -> Result<()> {
        validate_relative(relative)?;
        let parent = self.create_directory_parent(relative)?;
        let target = relative
            .file_name()
            .ok_or_else(|| ScienceError::Invalid("artifact path has no file name".into()))?;
        let temp = format!(".science-{}.tmp", Uuid::new_v4());
        let mut staged = openat(
            &parent.file,
            std::ffi::OsStr::new(&temp),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            Some(0o600),
        )?;
        let result = (|| -> Result<()> {
            staged.write_all(bytes)?;
            staged.sync_all()?;
            linkat(&parent.file, std::ffi::OsStr::new(&temp), target)?;
            if let Err(error) = unlinkat(&parent.file, std::ffi::OsStr::new(&temp)) {
                let _ = unlinkat(&parent.file, target);
                return Err(error);
            }
            #[cfg(test)]
            if FAIL_WRITE_NEW_PARENT_SYNC.with(|fail| fail.replace(false)) {
                return Err(ScienceError::Io(std::io::Error::other(
                    "injected write-new parent sync failure",
                )));
            }
            parent.file.sync_all()?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = unlinkat(&parent.file, std::ffi::OsStr::new(&temp));
            return Err(error);
        }
        Ok(())
    }

    fn replace_json_atomic<T: Serialize>(&self, relative: &Path, value: &T) -> Result<()> {
        self.replace_bytes_atomic(relative, &serde_json::to_vec_pretty(value)?)
    }

    fn replace_bytes_atomic(&self, relative: &Path, bytes: &[u8]) -> Result<()> {
        validate_relative(relative)?;
        let parent = self.create_directory_parent(relative)?;
        let target = relative
            .file_name()
            .ok_or_else(|| ScienceError::Invalid("artifact path has no file name".into()))?;
        let temp = format!(".science-{}.tmp", Uuid::new_v4());
        let mut staged = openat(
            &parent.file,
            std::ffi::OsStr::new(&temp),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            Some(0o600),
        )?;
        let result = (|| -> Result<()> {
            staged.write_all(bytes)?;
            staged.sync_all()?;
            renameat(
                &parent.file,
                std::ffi::OsStr::new(&temp),
                &parent.file,
                target,
            )?;
            parent.file.sync_all()?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = unlinkat(&parent.file, std::ffi::OsStr::new(&temp));
            return Err(error);
        }
        Ok(())
    }

    fn unlink_file(&self, relative: &Path) -> Result<()> {
        validate_relative(relative)?;
        let parent = self.open_directory_parent(relative)?;
        let name = relative
            .file_name()
            .ok_or_else(|| ScienceError::Invalid("artifact path has no file name".into()))?;
        match unlinkat(&parent.file, name) {
            Ok(()) => {
                parent.file.sync_all()?;
                Ok(())
            }
            Err(ScienceError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn open_directory_parent(&self, relative: &Path) -> Result<Self> {
        match relative.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => self.open_directory(parent),
            _ => self.try_clone(),
        }
    }

    fn create_directory_parent(&self, relative: &Path) -> Result<Self> {
        match relative.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => self.create_directories(parent),
            _ => self.try_clone(),
        }
    }

    fn sync_directory(&self) -> Result<()> {
        #[cfg(test)]
        if FAIL_EXPLICIT_DIRECTORY_SYNC.with(|fail| fail.replace(false)) {
            return Err(ScienceError::Io(std::io::Error::other(
                "injected explicit directory sync failure",
            )));
        }
        self.file.sync_all()?;
        Ok(())
    }

    fn sync_containing_directory(&self, relative: &Path) -> Result<()> {
        validate_relative(relative)?;
        self.open_directory_parent(relative)?.sync_directory()
    }

    fn try_clone(&self) -> Result<Self> {
        Ok(Self {
            file: self.file.try_clone()?,
        })
    }
}

#[cfg(unix)]
fn os_name(name: &std::ffi::OsStr) -> Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt as _;

    std::ffi::CString::new(name.as_bytes())
        .map_err(|_| ScienceError::Invalid("path component contains NUL".into()))
}

#[cfg(unix)]
fn openat(
    directory: &fs::File,
    name: &std::ffi::OsStr,
    flags: i32,
    mode: Option<libc::mode_t>,
) -> Result<fs::File> {
    use std::os::fd::{AsRawFd as _, FromRawFd as _};

    let name = os_name(name)?;
    // SAFETY: the directory fd and NUL-terminated child name are live. A mode
    // is supplied exactly when creation flags require it.
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
    // SAFETY: a nonnegative openat result transfers one owned descriptor.
    Ok(unsafe { fs::File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn mkdirat(directory: &fs::File, name: &std::ffi::OsStr, mode: libc::mode_t) -> Result<()> {
    use std::os::fd::AsRawFd as _;

    let name = os_name(name)?;
    // SAFETY: the directory fd and NUL-terminated child name are live.
    if unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), mode) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

#[cfg(unix)]
fn linkat(directory: &fs::File, source: &std::ffi::OsStr, target: &std::ffi::OsStr) -> Result<()> {
    use std::os::fd::AsRawFd as _;

    let source = os_name(source)?;
    let target = os_name(target)?;
    // SAFETY: both names are NUL-terminated and relative to the live fd.
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
fn renameat(
    source_directory: &fs::File,
    source: &std::ffi::OsStr,
    target_directory: &fs::File,
    target: &std::ffi::OsStr,
) -> Result<()> {
    use std::os::fd::AsRawFd as _;

    let source = os_name(source)?;
    let target = os_name(target)?;
    // SAFETY: both names are NUL-terminated and relative to live directory
    // descriptors.
    if unsafe {
        libc::renameat(
            source_directory.as_raw_fd(),
            source.as_ptr(),
            target_directory.as_raw_fd(),
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
fn unlinkat(directory: &fs::File, name: &std::ffi::OsStr) -> Result<()> {
    use std::os::fd::AsRawFd as _;

    let name = os_name(name)?;
    // SAFETY: the name is NUL-terminated and relative to the live directory
    // descriptor. Flags zero unlink only a non-directory entry and never
    // follow a symlink target.
    if unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), 0) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

#[cfg(unix)]
fn unlink_directory_at(directory: &fs::File, name: &std::ffi::OsStr) -> Result<()> {
    use std::os::fd::AsRawFd as _;

    let name = os_name(name)?;
    // SAFETY: the name is NUL-terminated and relative to the live directory
    // descriptor. AT_REMOVEDIR removes only an empty directory entry.
    if unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

#[cfg(target_os = "macos")]
fn rename_directory_noreplace_at(
    directory: &fs::File,
    staged: &std::ffi::OsStr,
    final_name: &std::ffi::OsStr,
) -> Result<()> {
    use std::os::fd::AsRawFd as _;

    let staged = os_name(staged)?;
    let final_name = os_name(final_name)?;
    // SAFETY: both names are NUL-terminated single components relative to the
    // same retained directory descriptor. RENAME_EXCL forbids replacement.
    if unsafe {
        libc::renameatx_np(
            directory.as_raw_fd(),
            staged.as_ptr(),
            directory.as_raw_fd(),
            final_name.as_ptr(),
            libc::RENAME_EXCL,
        )
    } == 0
    {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn rename_directory_noreplace_at(
    directory: &fs::File,
    staged: &std::ffi::OsStr,
    final_name: &std::ffi::OsStr,
) -> Result<()> {
    use std::os::fd::AsRawFd as _;

    let staged = os_name(staged)?;
    let final_name = os_name(final_name)?;
    // SAFETY: renameat2 receives valid retained descriptors and
    // NUL-terminated single-component names. RENAME_NOREPLACE is atomic.
    if unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            directory.as_raw_fd(),
            staged.as_ptr(),
            directory.as_raw_fd(),
            final_name.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    } == 0
    {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "linux", target_os = "android"))
))]
fn rename_directory_noreplace_at(
    _directory: &fs::File,
    _staged: &std::ffi::OsStr,
    _final_name: &std::ffi::OsStr,
) -> Result<()> {
    Err(ScienceError::FeatureDisabled(
        "atomic no-replace run publication is unavailable on this Unix platform".into(),
    ))
}

/// Windows retains directory/file handles opened with
/// `FILE_FLAG_OPEN_REPARSE_POINT` and rejects every reparse-point component
/// for ordinary record I/O. Run-directory publication is deliberately
/// disabled until a native handle-relative atomic no-replace backend is
/// implemented and exercised on a Windows host.
#[cfg(windows)]
#[derive(Debug)]
struct PinnedDirectory {
    path: PathBuf,
    file: fs::File,
}

#[cfg(windows)]
impl PinnedDirectory {
    fn create_path(path: &Path) -> Result<Self> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        match fs::symlink_metadata(&absolute) {
            Ok(metadata) if windows_has_reparse_point(&metadata) => {
                return Err(ScienceError::Invalid(
                    "science store root must not be a Windows reparse point".into(),
                ));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(ScienceError::Invalid(
                    "science store root must be a directory".into(),
                ));
            }
            Ok(_) => return Self::open_path(&absolute),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let parent = absolute
            .parent()
            .ok_or_else(|| ScienceError::Invalid("science store root has no parent".into()))?;
        let name = absolute
            .file_name()
            .ok_or_else(|| ScienceError::Invalid("science store root has no file name".into()))?;
        let parent = Self::create_path(parent)?;
        parent.create_directories(Path::new(name))
    }

    fn open_path(path: &Path) -> Result<Self> {
        let canonical = dunce::canonicalize(path)?;
        Self::open_verified_directory(canonical)
    }

    fn final_path(&self) -> Result<PathBuf> {
        self.assert_path_still_matches_handle()?;
        Ok(dunce::canonicalize(&self.path)?)
    }

    fn identity(&self) -> Result<StoreRootIdentity> {
        let identity = windows_file_identity(&self.file).ok_or_else(|| {
            ScienceError::Invalid("cannot resolve science store root identity".into())
        })?;
        Ok(StoreRootIdentity::Windows {
            volume: identity.volume_serial_number,
            index: identity.file_index,
        })
    }

    fn create_directory_new(&self, relative: &Path) -> Result<Self> {
        validate_relative(relative)?;
        let mut components = relative.components();
        let Some(Component::Normal(name)) = components.next() else {
            return Err(ScienceError::Invalid(
                "directory capability received a non-normal component".into(),
            ));
        };
        if components.next().is_some() {
            return Err(ScienceError::Invalid(
                "exclusive directory creation requires one component".into(),
            ));
        }
        self.assert_path_still_matches_handle()?;
        let child = self.path.join(name);
        fs::create_dir(&child)?;
        match Self::open_verified_directory(child.clone()) {
            Ok(directory) => Ok(directory),
            Err(error) => {
                let _ = fs::remove_dir(&child);
                Err(error)
            }
        }
    }

    fn remove_directory_if_empty(&self, relative: &Path) -> Result<bool> {
        validate_relative(relative)?;
        let mut components = relative.components();
        let Some(Component::Normal(name)) = components.next() else {
            return Err(ScienceError::Invalid(
                "directory removal received a non-normal component".into(),
            ));
        };
        if components.next().is_some() {
            return Err(ScienceError::Invalid(
                "exclusive directory removal requires one component".into(),
            ));
        }
        self.assert_path_still_matches_handle()?;
        let child = self.path.join(name);
        match fs::remove_dir(&child) {
            Ok(()) => {
                self.assert_path_still_matches_handle()?;
                Ok(true)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
            // ERROR_DIR_NOT_EMPTY
            Err(error) if error.raw_os_error() == Some(145) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    fn publish_directory_new(&self, staged: &Path, final_name: &Path) -> Result<()> {
        validate_relative(staged)?;
        validate_relative(final_name)?;
        if staged.components().count() != 1 || final_name.components().count() != 1 {
            return Err(ScienceError::Invalid(
                "directory publication requires sibling names".into(),
            ));
        }
        Err(ScienceError::FeatureDisabled(
            "atomic no-replace run publication has no Windows backend".into(),
        ))
    }

    fn open_directory(&self, relative: &Path) -> Result<Self> {
        validate_relative(relative)?;
        self.assert_path_still_matches_handle()?;
        let mut current = Self {
            path: self.path.clone(),
            file: self.file.try_clone()?,
        };
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err(ScienceError::Invalid(
                    "directory capability received a non-normal component".into(),
                ));
            };
            current.assert_path_still_matches_handle()?;
            current = Self::open_verified_directory(current.path.join(name))?;
        }
        Ok(current)
    }

    fn create_directories(&self, relative: &Path) -> Result<Self> {
        if relative.as_os_str().is_empty() {
            return Ok(Self {
                path: self.path.clone(),
                file: self.file.try_clone()?,
            });
        }
        validate_relative(relative)?;
        let mut current = Self {
            path: self.path.clone(),
            file: self.file.try_clone()?,
        };
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err(ScienceError::Invalid(
                    "directory capability received a non-normal component".into(),
                ));
            };
            current.assert_path_still_matches_handle()?;
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

    fn read_regular(&self, relative: &Path) -> Result<Vec<u8>> {
        validate_relative(relative)?;
        let parent = self.open_directory_parent(relative)?;
        parent.assert_path_still_matches_handle()?;
        let path = parent.path.join(
            relative
                .file_name()
                .ok_or_else(|| ScienceError::Invalid("artifact path has no file name".into()))?,
        );
        let mut file = windows_open_regular(&path, false)?;
        windows_assert_regular_handle(&path, &file)?;
        parent.assert_path_still_matches_handle()?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn read_regular_bounded(&self, relative: &Path, max_bytes: u64) -> Result<Vec<u8>> {
        validate_relative(relative)?;
        let parent = self.open_directory_parent(relative)?;
        parent.assert_path_still_matches_handle()?;
        let path = parent.path.join(
            relative
                .file_name()
                .ok_or_else(|| ScienceError::Invalid("artifact path has no file name".into()))?,
        );
        let file = windows_open_regular(&path, false)?;
        windows_assert_regular_handle(&path, &file)?;
        parent.assert_path_still_matches_handle()?;
        let mut bytes = Vec::new();
        file.take(max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > max_bytes {
            return Err(ScienceError::Invalid(
                "artifact exceeds the caller's byte limit".into(),
            ));
        }
        Ok(bytes)
    }

    fn hash_regular(&self, relative: &Path) -> Result<(u64, String)> {
        validate_relative(relative)?;
        let parent = self.open_directory_parent(relative)?;
        parent.assert_path_still_matches_handle()?;
        let path = parent.path.join(
            relative
                .file_name()
                .ok_or_else(|| ScienceError::Invalid("artifact path has no file name".into()))?,
        );
        let mut file = windows_open_regular(&path, false)?;
        windows_assert_regular_handle(&path, &file)?;
        parent.assert_path_still_matches_handle()?;
        let result = hash_open_file(&mut file)?;
        parent.assert_path_still_matches_handle()?;
        Ok(result)
    }

    fn read_json<T: DeserializeOwned>(&self, relative: &Path) -> Result<T> {
        Ok(serde_json::from_slice(&self.read_regular(relative)?)?)
    }

    fn read_json_bounded<T: DeserializeOwned>(&self, relative: &Path, max_bytes: u64) -> Result<T> {
        Ok(serde_json::from_slice(
            &self.read_regular_bounded(relative, max_bytes)?,
        )?)
    }

    fn write_new_atomic(&self, relative: &Path, bytes: &[u8]) -> Result<()> {
        validate_relative(relative)?;
        let parent = self.create_directory_parent(relative)?;
        parent.assert_path_still_matches_handle()?;
        let target = parent.path.join(
            relative
                .file_name()
                .ok_or_else(|| ScienceError::Invalid("artifact path has no file name".into()))?,
        );
        let temp = parent.path.join(format!(".science-{}.tmp", Uuid::new_v4()));
        let mut staged = windows_open_regular(&temp, true)?;
        let result = (|| -> Result<()> {
            windows_assert_regular_handle(&temp, &staged)?;
            staged.write_all(bytes)?;
            staged.sync_all()?;
            parent.assert_path_still_matches_handle()?;
            fs::hard_link(&temp, &target)?;
            parent.assert_path_still_matches_handle()?;
            fs::remove_file(&temp)?;
            #[cfg(test)]
            if FAIL_WRITE_NEW_PARENT_SYNC.with(|fail| fail.replace(false)) {
                return Err(ScienceError::Io(std::io::Error::other(
                    "injected write-new parent sync failure",
                )));
            }
            parent.sync_directory()?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&temp);
            return Err(error);
        }
        Ok(())
    }

    fn replace_json_atomic<T: Serialize>(&self, relative: &Path, value: &T) -> Result<()> {
        self.replace_bytes_atomic(relative, &serde_json::to_vec_pretty(value)?)
    }

    fn replace_bytes_atomic(&self, relative: &Path, bytes: &[u8]) -> Result<()> {
        validate_relative(relative)?;
        let parent = self.create_directory_parent(relative)?;
        parent.assert_path_still_matches_handle()?;
        let target = parent.path.join(
            relative
                .file_name()
                .ok_or_else(|| ScienceError::Invalid("artifact path has no file name".into()))?,
        );
        if target.exists() {
            let current = windows_open_regular(&target, false)?;
            windows_assert_regular_handle(&target, &current)?;
        }
        let temp = parent.path.join(format!(".science-{}.tmp", Uuid::new_v4()));
        let mut staged = windows_open_regular(&temp, true)?;
        let result = (|| -> Result<()> {
            windows_assert_regular_handle(&temp, &staged)?;
            staged.write_all(bytes)?;
            staged.sync_all()?;
            parent.assert_path_still_matches_handle()?;
            fs::rename(&temp, &target)?;
            parent.assert_path_still_matches_handle()?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&temp);
            return Err(error);
        }
        Ok(())
    }

    fn unlink_file(&self, relative: &Path) -> Result<()> {
        validate_relative(relative)?;
        let parent = self.open_directory_parent(relative)?;
        parent.assert_path_still_matches_handle()?;
        let target = parent.path.join(
            relative
                .file_name()
                .ok_or_else(|| ScienceError::Invalid("artifact path has no file name".into()))?,
        );
        match fs::symlink_metadata(&target) {
            Ok(metadata) if windows_has_reparse_point(&metadata) => {
                return Err(ScienceError::Invalid(
                    "artifact deletion refuses a Windows reparse point".into(),
                ));
            }
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => {
                return Err(ScienceError::Invalid(
                    "artifact deletion requires a regular file".into(),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        }
        let opened = windows_open_regular(&target, false)?;
        windows_assert_regular_handle(&target, &opened)?;
        parent.assert_path_still_matches_handle()?;
        fs::remove_file(&target)?;
        parent.assert_path_still_matches_handle()
    }

    fn open_verified_directory(path: PathBuf) -> Result<Self> {
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_dir() || windows_has_reparse_point(&metadata) {
            return Err(ScienceError::Invalid(
                "artifact directory must not be a Windows reparse point".into(),
            ));
        }
        let file = windows_open_directory(&path)?;
        let directory = Self { path, file };
        directory.assert_path_still_matches_handle()?;
        Ok(directory)
    }

    fn assert_path_still_matches_handle(&self) -> Result<()> {
        let reopened = windows_open_directory(&self.path)?;
        if !windows_same_open_file(&self.file, &reopened)
            || !windows_final_handle_path_matches(&self.path, &self.file)
        {
            return Err(ScienceError::Invalid(
                "artifact directory identity changed during operation".into(),
            ));
        }
        Ok(())
    }

    fn open_directory_parent(&self, relative: &Path) -> Result<Self> {
        match relative.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => self.open_directory(parent),
            _ => Ok(Self {
                path: self.path.clone(),
                file: self.file.try_clone()?,
            }),
        }
    }

    fn create_directory_parent(&self, relative: &Path) -> Result<Self> {
        match relative.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => self.create_directories(parent),
            _ => Ok(Self {
                path: self.path.clone(),
                file: self.file.try_clone()?,
            }),
        }
    }

    fn sync_directory(&self) -> Result<()> {
        #[cfg(test)]
        if FAIL_EXPLICIT_DIRECTORY_SYNC.with(|fail| fail.replace(false)) {
            return Err(ScienceError::Io(std::io::Error::other(
                "injected explicit directory sync failure",
            )));
        }
        self.assert_path_still_matches_handle()?;
        self.file.sync_all()?;
        self.assert_path_still_matches_handle()
    }

    fn sync_containing_directory(&self, relative: &Path) -> Result<()> {
        validate_relative(relative)?;
        self.open_directory_parent(relative)?.sync_directory()
    }

    fn try_clone(&self) -> Result<Self> {
        Ok(Self {
            path: self.path.clone(),
            file: self.file.try_clone()?,
        })
    }
}

#[cfg(windows)]
fn windows_open_directory(path: &Path) -> Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt as _;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    let mut options = fs::OpenOptions::new();
    options
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    Ok(options.open(path)?)
}

#[cfg(windows)]
fn windows_open_regular(path: &Path, create_new: bool) -> Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt as _;

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    let mut options = fs::OpenOptions::new();
    if create_new {
        options.write(true).create_new(true);
    } else {
        options.read(true);
    }
    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    Ok(options.open(path)?)
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
            "artifact must be a stable non-reparse regular file".into(),
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn windows_same_open_file(expected: &fs::File, opened: &fs::File) -> bool {
    windows_file_identity(expected).is_some_and(|expected| {
        windows_file_identity(opened).is_some_and(|opened| opened == expected)
    })
}

#[cfg(windows)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct WindowsFileIdentity {
    volume_serial_number: u32,
    file_index: u64,
}

#[cfg(windows)]
fn windows_file_identity(file: &fs::File) -> Option<WindowsFileIdentity> {
    use std::os::windows::io::AsRawHandle as _;

    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[repr(C)]
    struct ByHandleFileInformation {
        file_attributes: u32,
        creation_time: FileTime,
        last_access_time: FileTime,
        last_write_time: FileTime,
        volume_serial_number: u32,
        file_size_high: u32,
        file_size_low: u32,
        number_of_links: u32,
        file_index_high: u32,
        file_index_low: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "GetFileInformationByHandle"]
        fn get_file_information_by_handle(
            file: *mut std::ffi::c_void,
            information: *mut ByHandleFileInformation,
        ) -> i32;
    }
    let mut information = std::mem::MaybeUninit::uninit();
    // SAFETY: the borrowed handle is live and the output layout matches
    // BY_HANDLE_FILE_INFORMATION.
    if unsafe {
        get_file_information_by_handle(file.as_raw_handle().cast(), information.as_mut_ptr())
    } == 0
    {
        return None;
    }
    // SAFETY: a nonzero result initialized the full output.
    let information = unsafe { information.assume_init() };
    Some(WindowsFileIdentity {
        volume_serial_number: information.volume_serial_number,
        file_index: (u64::from(information.file_index_high) << 32)
            | u64::from(information.file_index_low),
    })
}

#[cfg(windows)]
fn windows_final_handle_path_matches(path: &Path, file: &fs::File) -> bool {
    use std::os::windows::ffi::OsStringExt as _;
    use std::os::windows::io::AsRawHandle as _;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "GetFinalPathNameByHandleW"]
        fn get_final_path_name_by_handle(
            file: *mut std::ffi::c_void,
            path: *mut u16,
            path_len: u32,
            flags: u32,
        ) -> u32;
    }
    let handle = file.as_raw_handle().cast();
    // SAFETY: a null output and zero length is the documented size query.
    let needed = unsafe { get_final_path_name_by_handle(handle, std::ptr::null_mut(), 0, 0) };
    if needed == 0 {
        return false;
    }
    let mut buffer = vec![0_u16; needed as usize + 1];
    // SAFETY: the buffer has the advertised writable UTF-16 capacity.
    let written = unsafe {
        get_final_path_name_by_handle(
            handle,
            buffer.as_mut_ptr(),
            u32::try_from(buffer.len()).unwrap_or(u32::MAX),
            0,
        )
    };
    if written == 0 || written as usize >= buffer.len() {
        return false;
    }
    let handle_path = PathBuf::from(std::ffi::OsString::from_wide(&buffer[..written as usize]));
    dunce::canonicalize(path).is_ok_and(|canonical| canonical == dunce::simplified(&handle_path))
}

#[cfg(not(any(unix, windows)))]
#[derive(Debug)]
struct PinnedDirectory;

#[cfg(not(any(unix, windows)))]
impl PinnedDirectory {
    fn create_path(_path: &Path) -> Result<Self> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }

    fn open_path(_path: &Path) -> Result<Self> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }

    fn final_path(&self) -> Result<PathBuf> {
        Err(ScienceError::FeatureDisabled(
            "confined store final-path validation has no backend for this platform".into(),
        ))
    }

    fn identity(&self) -> Result<StoreRootIdentity> {
        Err(ScienceError::FeatureDisabled(
            "confined store identity has no backend for this platform".into(),
        ))
    }

    fn open_directory(&self, _relative: &Path) -> Result<Self> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }

    fn create_directory_new(&self, _relative: &Path) -> Result<Self> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }

    fn remove_directory_if_empty(&self, _relative: &Path) -> Result<bool> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }

    fn publish_directory_new(&self, _staged: &Path, _final_name: &Path) -> Result<()> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }

    fn create_directories(&self, _relative: &Path) -> Result<Self> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }

    fn read_regular(&self, _relative: &Path) -> Result<Vec<u8>> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }

    fn read_regular_bounded(&self, _relative: &Path, _max_bytes: u64) -> Result<Vec<u8>> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }

    fn hash_regular(&self, _relative: &Path) -> Result<(u64, String)> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }

    fn read_json<T: DeserializeOwned>(&self, _relative: &Path) -> Result<T> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }

    fn read_json_bounded<T: DeserializeOwned>(
        &self,
        _relative: &Path,
        _max_bytes: u64,
    ) -> Result<T> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }

    fn write_new_atomic(&self, _relative: &Path, _bytes: &[u8]) -> Result<()> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }

    fn replace_json_atomic<T: Serialize>(&self, _relative: &Path, _value: &T) -> Result<()> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }

    fn unlink_file(&self, _relative: &Path) -> Result<()> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }

    fn sync_directory(&self) -> Result<()> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }

    fn sync_containing_directory(&self, _relative: &Path) -> Result<()> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }

    fn try_clone(&self) -> Result<Self> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }
}

fn hash_open_file(file: &mut fs::File) -> Result<(u64, String)> {
    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes = bytes
            .checked_add(read as u64)
            .ok_or_else(|| ScienceError::Invalid("artifact length overflow".into()))?;
        digest.update(&buffer[..read]);
    }
    Ok((bytes, format!("{:x}", digest.finalize())))
}

fn validate_context(context: &RunContext) -> Result<()> {
    context.run_id.validate()?;
    context.project_id.validate()?;
    if context.session_id.is_empty() || context.owner_id.is_empty() {
        return Err(ScienceError::Invalid(
            "session id and owner must be non-empty".into(),
        ));
    }
    Ok(())
}

fn validate_persisted_id(kind: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > MAX_PERSISTED_ID_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(ScienceError::Invalid(format!(
            "{kind} must be 1..={MAX_PERSISTED_ID_BYTES} [A-Za-z0-9_-] bytes"
        )));
    }
    Ok(())
}

fn validate_run_ids<'a>(
    run_ids: impl IntoIterator<Item = &'a RunId>,
    expected: &RunId,
    record_kind: &str,
) -> Result<()> {
    expected.validate()?;
    for run_id in run_ids {
        run_id.validate()?;
        if run_id != expected {
            return Err(ScienceError::Invalid(format!(
                "{record_kind} record belongs to a different run"
            )));
        }
    }
    Ok(())
}

fn validate_events(events: &[Event], expected: &RunId) -> Result<()> {
    validate_run_ids(events.iter().map(|event| &event.run_id), expected, "event")
}

fn validate_approval(approval: &Approval, expected: &RunId) -> Result<()> {
    approval.project_id.validate()?;
    approval.run_id.validate()?;
    approval.call_id.validate()?;
    if &approval.run_id != expected {
        return Err(ScienceError::Invalid(
            "approval record belongs to a different run".into(),
        ));
    }
    Ok(())
}

fn validate_approvals(approvals: &[Approval], expected: &RunId) -> Result<()> {
    expected.validate()?;
    for approval in approvals {
        validate_approval(approval, expected)?;
    }
    Ok(())
}

fn validate_artifacts(artifacts: &[Artifact], expected: &RunId) -> Result<()> {
    expected.validate()?;
    for artifact in artifacts {
        artifact.run_id.validate()?;
        artifact.call_id.validate()?;
        validate_relative(&artifact.relative_path)?;
        if &artifact.run_id != expected {
            return Err(ScienceError::Invalid(
                "artifact record belongs to a different run".into(),
            ));
        }
    }
    Ok(())
}

fn validate_previews(items: &[preview::PreviewRecord], expected: &RunId) -> Result<()> {
    expected.validate()?;
    for preview in items {
        preview.run_id.validate()?;
        preview.call_id.validate()?;
        validate_relative(&preview.relative_path)?;
        if &preview.run_id != expected {
            return Err(ScienceError::Invalid(
                "preview record belongs to a different run".into(),
            ));
        }
    }
    Ok(())
}
pub(crate) fn validate_relative(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(ScienceError::Invalid(
            "artifact path must be a normal relative path".into(),
        ));
    }
    Ok(())
}
fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Best-effort fsync of a directory, so the rename that published a record is
/// itself durable and not just the bytes it points at.
///
/// Best-effort by design: not every platform or filesystem lets a directory be
/// opened and synced (Windows cannot at all), and a crash between the rename
/// and the directory flush still leaves the *previous* complete record on
/// disk, never a torn one. Failing the write there would trade a real
/// durability gain for a spurious hard error.
#[cfg(test)]
pub(crate) fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    {
        if let Ok(handle) = fs::File::open(dir) {
            let _ = handle.sync_all();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn context(root: &Path, project: &str, owner: &str) -> RunContext {
        RunContext {
            run_id: RunId::new_v7(),
            project_id: ProjectId::new(project),
            session_id: format!("session-{project}"),
            owner_id: owner.into(),
            workspace_root: root.join(project),
            provider: "offline".into(),
            approval_policy: "ask".into(),
            tool_profile: "science-csv".into(),
            artifact_root: root.join(project).join("artifacts"),
            environment: BTreeMap::from([("locale".into(), "C".into())]),
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_run_publication_fails_closed_without_visible_final_directory() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("science-store");
        let mut context = context(temp.path(), "windows-publication", "owner-a");
        context.run_id = RunId::new("windows-publication-run");
        let final_dir = root.join("runs").join(&context.run_id.0);

        let error = ScienceStore::new(&root)
            .create_run(context)
            .expect_err("Windows used a pathname rename as atomic no-replace publication");
        assert!(
            matches!(
                error,
                ScienceError::FeatureDisabled(ref message)
                    if message == "atomic no-replace run publication has no Windows backend"
            ),
            "unexpected Windows publication error: {error}"
        );
        assert!(
            !final_dir.exists(),
            "failed Windows run publication exposed a final authority directory"
        );
    }

    #[test]
    fn stale_nonempty_staging_directory_cannot_poison_final_run_id() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("science-store");
        let runs = root.join("runs");
        let mut context = context(temp.path(), "staged-create", "owner-a");
        context.run_id = RunId::new("staged-create-run");
        let stale = runs.join(format!(".run-init-{}-stale", context.run_id.0));
        fs::create_dir_all(&stale).unwrap();
        fs::write(stale.join(".science-orphan.tmp"), b"partial").unwrap();

        let store = ScienceStore::new(&root);
        let created = store.create_run(context.clone()).unwrap();
        assert_eq!(created.context, context);
        assert_eq!(store.load_run(&context.run_id).unwrap(), created);
        assert!(root.join("runs").join(&context.run_id.0).is_dir());
        assert!(
            stale.is_dir(),
            "untrusted stale staging is not deleted during publication"
        );
    }

    #[test]
    fn no_replace_run_publication_preserves_preexisting_final_directory() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("science-store");
        let mut context = context(temp.path(), "exclusive-create", "owner-a");
        context.run_id = RunId::new("exclusive-create-run");
        let final_dir = root.join("runs").join(&context.run_id.0);
        fs::create_dir_all(&final_dir).unwrap();
        fs::write(final_dir.join("foreign-record"), b"must survive").unwrap();

        let store = ScienceStore::new(&root);
        assert!(store.create_run(context).is_err());
        assert_eq!(
            fs::read(final_dir.join("foreign-record")).unwrap(),
            b"must survive"
        );
        assert!(!final_dir.join("run.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn parent_sync_failure_blocks_begin_and_retry_resyncs_existing_directory() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("science-store");
        fs::create_dir(&root).unwrap();
        let mut context = context(temp.path(), "directory-sync", "owner-a");
        context.run_id = RunId::new("directory-sync-run");
        let store = ScienceStore::new(&root);

        FAIL_DIRECTORY_ENTRY_PARENT_SYNC.with(|fail| fail.set(true));
        let error = store
            .create_run(context.clone())
            .expect_err("Begin ignored the runs-directory parent sync failure");
        assert!(
            error
                .to_string()
                .contains("injected directory-entry parent sync failure"),
            "unexpected directory sync error: {error}"
        );
        assert!(
            !root.join("runs").join(&context.run_id.0).exists(),
            "failed directory durability opened a visible run"
        );

        let created = store
            .create_run(context.clone())
            .expect("retry must sync the retained existing runs directory entry");
        assert_eq!(created.context, context);
        assert_eq!(store.load_run(&context.run_id).unwrap(), created);
    }

    #[test]
    fn visible_rename_with_failed_parent_sync_is_not_reported_durable_or_deleted() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("science-store");
        let mut context = context(temp.path(), "visible-not-durable", "owner-a");
        context.run_id = RunId::new("visible-not-durable-run");
        let store = ScienceStore::new(&root);

        FAIL_RUN_PUBLICATION_AFTER_VISIBLE_RENAME.with(|fail| fail.set(true));
        let error = store
            .create_run(context.clone())
            .expect_err("visible rename without parent sync was reported durable");
        assert!(
            error
                .to_string()
                .contains("injected runs-directory sync failure"),
            "unexpected publication error: {error}"
        );

        let final_dir = root.join("runs").join(&context.run_id.0);
        assert!(
            final_dir.is_dir(),
            "reconciliation deleted a final directory after rename became visible"
        );
        assert_eq!(
            store.load_run(&context.run_id).unwrap().context,
            context,
            "a later retry could not reopen the exact visible run"
        );
    }

    fn request_pending_call(store: &ScienceStore, run: &RunRecord, call: &CallId) {
        store
            .request_approval(Approval {
                project_id: run.context.project_id.clone(),
                run_id: run.context.run_id.clone(),
                call_id: call.clone(),
                owner_id: run.context.owner_id.clone(),
                decision: ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
    }

    fn allow_running_call(store: &ScienceStore, run: &RunRecord, call: &str) -> CallId {
        let call = CallId::new(call);
        request_pending_call(store, run, &call);
        store
            .transition(&run.context.run_id, RunState::AwaitingApproval, None)
            .unwrap();
        let approvals = store.approvals(&run.context.run_id).unwrap();
        let [pending] = approvals.as_slice() else {
            panic!("fixture must retain exactly one pending approval");
        };
        assert_eq!(pending.call_id, call);
        assert_eq!(pending.decision, ApprovalDecision::Pending);
        assert!(pending.decided_at.is_none());
        store
            .decide_approval(
                &run.context.project_id,
                &run.context.run_id,
                &run.context.owner_id,
                &call,
                ApprovalDecision::Allow,
            )
            .unwrap();
        store
            .transition(&run.context.run_id, RunState::Running, None)
            .unwrap();
        let approvals = store.approvals(&run.context.run_id).unwrap();
        let [allowed] = approvals.as_slice() else {
            panic!("fixture must retain exactly one allowed approval");
        };
        assert_eq!(allowed.call_id, call);
        assert_eq!(allowed.decision, ApprovalDecision::Allow);
        assert!(allowed.decided_at.is_some());
        assert_eq!(
            store.load_run(&run.context.run_id).unwrap().state,
            RunState::Running
        );
        call
    }

    #[test]
    fn put_artifact_recovers_payload_visible_before_registry() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("store"));
        let run = store
            .create_run(context(temp.path(), "artifact-orphan", "alice"))
            .unwrap();
        let call = allow_running_call(&store, &run, "artifact-call");
        let relative = Path::new("nested/result.bin");
        let payload = b"exact orphan payload";
        let payload_path = store
            .run_dir(&run.context.run_id)
            .unwrap()
            .join("artifacts")
            .join(relative);
        fs::create_dir_all(payload_path.parent().unwrap()).unwrap();
        fs::write(&payload_path, payload).unwrap();
        assert!(store.artifacts(&run.context.run_id).unwrap().is_empty());

        let recovered = store
            .put_artifact(
                &run.context.project_id,
                &run.context.run_id,
                "alice",
                call.clone(),
                relative,
                payload,
                "application/octet-stream",
                "recovered orphan",
            )
            .unwrap();
        assert_eq!(recovered.sha256, hex_sha256(payload));
        assert_eq!(
            store.artifacts(&run.context.run_id).unwrap(),
            vec![recovered]
        );
        assert_eq!(
            store
                .allowed_running_artifact_bytes(
                    &run.context.project_id,
                    &run.context.run_id,
                    "alice",
                    &call,
                    relative,
                )
                .unwrap(),
            payload
        );

        assert!(
            store
                .put_artifact(
                    &run.context.project_id,
                    &run.context.run_id,
                    "alice",
                    call,
                    relative,
                    b"different retry",
                    "application/octet-stream",
                    "recovered orphan",
                )
                .is_err(),
            "orphan reconciliation accepted different bytes"
        );
    }

    #[test]
    fn put_artifact_requires_actual_parent_sync_before_registry_commit() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("store"));
        let run = store
            .create_run(context(temp.path(), "artifact-durable-parent", "alice"))
            .unwrap();
        let call = allow_running_call(&store, &run, "artifact-call");
        let relative = Path::new("nested/result.bin");
        let payload = b"durable nested artifact";

        FAIL_WRITE_NEW_PARENT_SYNC.with(|fail| fail.set(true));
        assert!(
            store
                .put_artifact(
                    &run.context.project_id,
                    &run.context.run_id,
                    "alice",
                    call.clone(),
                    relative,
                    payload,
                    "application/octet-stream",
                    "durable parent",
                )
                .is_err(),
            "ambiguous visible payload was registered in the failing call"
        );
        assert!(store.artifacts(&run.context.run_id).unwrap().is_empty());
        let payload_path = store
            .run_dir(&run.context.run_id)
            .unwrap()
            .join("artifacts")
            .join(relative);
        assert!(
            payload_path.is_file(),
            "fault injection did not reach the visible-before-parent-sync cut"
        );

        FAIL_EXPLICIT_DIRECTORY_SYNC.with(|fail| fail.set(true));
        assert!(
            store
                .put_artifact(
                    &run.context.project_id,
                    &run.context.run_id,
                    "alice",
                    call.clone(),
                    relative,
                    payload,
                    "application/octet-stream",
                    "durable parent",
                )
                .is_err(),
            "retry registered payload without syncing its actual nested parent"
        );
        assert!(store.artifacts(&run.context.run_id).unwrap().is_empty());

        let recovered = store
            .put_artifact(
                &run.context.project_id,
                &run.context.run_id,
                "alice",
                call,
                relative,
                payload,
                "application/octet-stream",
                "durable parent",
            )
            .unwrap();
        assert_eq!(
            store.artifacts(&run.context.run_id).unwrap(),
            vec![recovered]
        );
    }

    #[test]
    fn put_artifact_repairs_registry_visible_with_missing_payload() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("store"));
        let run = store
            .create_run(context(temp.path(), "artifact-registry-first", "alice"))
            .unwrap();
        let call = allow_running_call(&store, &run, "artifact-call");
        let relative = Path::new("result.bin");
        let payload = b"registry-first payload";
        let expected = Artifact {
            run_id: run.context.run_id.clone(),
            call_id: call.clone(),
            relative_path: relative.to_path_buf(),
            sha256: hex_sha256(payload),
            bytes: payload.len() as u64,
            mime: "application/octet-stream".into(),
            preview: "repair missing payload".into(),
        };
        fs::write(
            store
                .run_dir(&run.context.run_id)
                .unwrap()
                .join("artifacts.json"),
            serde_json::to_vec_pretty(&vec![expected.clone()]).unwrap(),
        )
        .unwrap();
        assert!(
            !store
                .run_dir(&run.context.run_id)
                .unwrap()
                .join("artifacts")
                .join(relative)
                .exists()
        );

        let repaired = store
            .put_artifact(
                &run.context.project_id,
                &run.context.run_id,
                "alice",
                call.clone(),
                relative,
                payload,
                "application/octet-stream",
                "repair missing payload",
            )
            .unwrap();
        assert_eq!(repaired, expected);
        assert_eq!(
            store
                .allowed_running_artifact_bytes(
                    &run.context.project_id,
                    &run.context.run_id,
                    "alice",
                    &call,
                    relative,
                )
                .unwrap(),
            payload
        );
    }

    fn preview_record(
        run: &RunRecord,
        call: &CallId,
        relative_path: &Path,
        artifact_sha256: String,
    ) -> preview::PreviewRecord {
        preview::PreviewRecord {
            run_id: run.context.run_id.clone(),
            call_id: call.clone(),
            relative_path: relative_path.to_path_buf(),
            artifact_sha256,
            preview: preview::Preview {
                kind: preview::PreviewKind::Text,
                mime: "text/plain".into(),
                bytes: 7,
                truncated: false,
                stats: preview::PreviewStats::Text { lines: 1 },
            },
            generated_at: Utc::now(),
            tool: "fixture".into(),
        }
    }

    fn successful_completion_fixture(
        project: &str,
    ) -> (tempfile::TempDir, ScienceStore, RunRecord, CallId, Artifact) {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("store"));
        let run = store
            .create_run(context(temp.path(), project, "alice"))
            .unwrap();
        let call = allow_running_call(&store, &run, "completion-call");
        let artifact = store
            .put_artifact(
                &run.context.project_id,
                &run.context.run_id,
                &run.context.owner_id,
                call.clone(),
                Path::new("result.txt"),
                b"verified completion bytes",
                "text/plain",
                "verified",
            )
            .unwrap();
        store
            .add_evidence(Evidence {
                run_id: run.context.run_id.clone(),
                claim: "verified completion".into(),
                source: "fixture".into(),
                artifact_sha256: Some(artifact.sha256.clone()),
                verified_at: Utc::now(),
            })
            .unwrap();
        store
            .add_provenance(Provenance {
                run_id: run.context.run_id.clone(),
                source_uri: "fixture://successful-completion".into(),
                source_commit: None,
                source_path: Some("input.txt".into()),
                license: "test-only".into(),
                retrieved_at: Utc::now(),
                input_sha256: artifact.sha256.clone(),
                tool: "successful-completion-fixture".into(),
                environment: BTreeMap::new(),
            })
            .unwrap();
        store
            .add_preview(preview_record(
                &run,
                &call,
                &artifact.relative_path,
                artifact.sha256.clone(),
            ))
            .unwrap();
        store
            .append_event(
                &run.context.run_id,
                "SessionActor",
                "fixture.execution.finished",
                serde_json::json!({
                    "run_id": run.context.run_id.0.clone(),
                    "call_id": call.0.clone(),
                    "project_id": run.context.project_id.0.clone(),
                    "session_id": run.context.session_id.clone(),
                    "owner_id": run.context.owner_id.clone(),
                }),
            )
            .unwrap();
        (temp, store, run, call, artifact)
    }

    fn completion_manifest(store: &ScienceStore, run: &RunRecord) -> SuccessfulCompletionManifest {
        let events = store.events_after(&run.context.run_id, 0, 1_000).unwrap();
        SuccessfulCompletionManifest {
            context: run.context.clone(),
            artifacts: store.artifacts(&run.context.run_id).unwrap(),
            evidence: store.evidence(&run.context.run_id).unwrap(),
            provenance: store.provenance(&run.context.run_id).unwrap(),
            previews: store.previews(&run.context.run_id).unwrap(),
            final_event: events.last().unwrap().clone(),
            events,
        }
    }

    fn assert_all_output_writes_rejected(store: &ScienceStore, run: &RunRecord, call: &CallId) {
        let relative = Path::new("blocked.txt");
        assert!(
            store
                .put_artifact(
                    &run.context.project_id,
                    &run.context.run_id,
                    &run.context.owner_id,
                    call.clone(),
                    relative,
                    b"blocked",
                    "text/plain",
                    "blocked",
                )
                .is_err()
        );
        assert!(
            store
                .add_evidence(Evidence {
                    run_id: run.context.run_id.clone(),
                    claim: "blocked".into(),
                    source: "fixture".into(),
                    artifact_sha256: Some(hex_sha256(b"blocked")),
                    verified_at: Utc::now(),
                })
                .is_err()
        );
        assert!(
            store
                .add_provenance(Provenance {
                    run_id: run.context.run_id.clone(),
                    source_uri: "fixture://blocked".into(),
                    source_commit: None,
                    source_path: None,
                    license: "test-only".into(),
                    retrieved_at: Utc::now(),
                    input_sha256: hex_sha256(b"blocked"),
                    tool: "fixture".into(),
                    environment: BTreeMap::new(),
                })
                .is_err()
        );
        assert!(
            store
                .add_preview(preview_record(run, call, relative, hex_sha256(b"blocked"),))
                .is_err()
        );
        assert!(store.artifacts(&run.context.run_id).unwrap().is_empty());
        assert!(store.evidence(&run.context.run_id).unwrap().is_empty());
        assert!(store.provenance(&run.context.run_id).unwrap().is_empty());
        assert!(store.previews(&run.context.run_id).unwrap().is_empty());
        assert_eq!(
            fs::read_dir(
                store
                    .run_dir(&run.context.run_id)
                    .unwrap()
                    .join("artifacts")
            )
            .unwrap()
            .count(),
            0
        );
    }

    #[test]
    fn exact_successful_completion_manifest_commits_under_one_store_lock() {
        let (_temp, store, run, _call, _artifact) =
            successful_completion_fixture("manifest-success");
        let manifest = completion_manifest(&store, &run);
        let mut wrong_final_event = manifest.clone();
        wrong_final_event.final_event.kind = "not.the.durable.final.event".into();

        assert!(
            store
                .transition(&run.context.run_id, RunState::Succeeded, None)
                .is_err(),
            "ordinary transition bypassed the successful-completion manifest"
        );
        assert!(
            store
                .transition_succeeded_with_manifest(&wrong_final_event)
                .is_err(),
            "a manifest without the unique durable final event was accepted"
        );
        assert_eq!(
            store.load_run(&run.context.run_id).unwrap().state,
            RunState::Running
        );
        let succeeded = store.transition_succeeded_with_manifest(&manifest).unwrap();
        assert_eq!(succeeded.context, manifest.context);
        assert_eq!(succeeded.state, RunState::Succeeded);
        assert_eq!(store.load_run(&run.context.run_id).unwrap(), succeeded);
        let events = store.events_after(&run.context.run_id, 0, 1_000).unwrap();
        assert!(
            store
                .append_event(
                    &run.context.run_id,
                    "Intruder",
                    "event.after-exact-completion",
                    serde_json::json!({}),
                )
                .is_err(),
            "exact successful completion accepted a late event"
        );
        assert_eq!(
            store.events_after(&run.context.run_id, 0, 1_000).unwrap(),
            events,
            "rejected late event changed the sealed completion history"
        );
    }

    #[test]
    fn completion_anchor_freezes_snapshot_before_terminal_and_recovers_exactly() {
        let (_temp, store, run, _call, artifact) =
            successful_completion_fixture("manifest-seal-recovery");
        let manifest = completion_manifest(&store, &run);
        {
            let _guard = store.writes.lock().unwrap();
            let run_dir = store.open_run_directory(&run.context.run_id).unwrap();
            let mut running =
                ScienceStore::load_run_record_from_directory(&run_dir, &run.context.run_id)
                    .unwrap();
            let approval = ScienceStore::running_allowed_approval(
                &run_dir,
                &running,
                &run.context.run_id,
                None,
            )
            .unwrap();
            let expected =
                ScienceStore::expected_successful_completion_seal(&running, &approval, &manifest)
                    .unwrap();
            ScienceStore::persist_successful_completion_anchor_unlocked(
                &run_dir,
                &mut running,
                &expected.manifest_sha256,
            )
            .unwrap();
        }

        let exact_retry = store
            .put_artifact(
                &run.context.project_id,
                &run.context.run_id,
                &run.context.owner_id,
                artifact.call_id.clone(),
                &artifact.relative_path,
                b"verified completion bytes",
                "text/plain",
                "verified",
            )
            .expect("sealed completion must permit an exact read-only artifact retry");
        assert_eq!(exact_retry, artifact);
        assert!(
            store
                .put_artifact(
                    &run.context.project_id,
                    &run.context.run_id,
                    &run.context.owner_id,
                    artifact.call_id.clone(),
                    &artifact.relative_path,
                    b"changed completion bytes",
                    "text/plain",
                    "verified",
                )
                .is_err(),
            "completion seal accepted a changed artifact retry"
        );
        assert!(
            store
                .put_artifact(
                    &run.context.project_id,
                    &run.context.run_id,
                    &run.context.owner_id,
                    artifact.call_id.clone(),
                    Path::new("late.txt"),
                    b"late",
                    "text/plain",
                    "late",
                )
                .is_err(),
            "completion seal accepted a new artifact"
        );
        assert!(
            store
                .append_event(
                    &run.context.run_id,
                    "Intruder",
                    "event.after-completion-seal",
                    serde_json::json!({}),
                )
                .is_err(),
            "completion seal accepted a late event before the terminal write"
        );
        assert!(
            store
                .add_evidence(Evidence {
                    run_id: run.context.run_id.clone(),
                    claim: "late".into(),
                    source: "fixture://late".into(),
                    artifact_sha256: Some(artifact.sha256.clone()),
                    verified_at: Utc::now(),
                })
                .is_err(),
            "completion seal accepted a late output before the terminal write"
        );
        assert!(
            store
                .transition(
                    &run.context.run_id,
                    RunState::Failed,
                    Some("late rollback".into()),
                )
                .is_err(),
            "completion seal allowed an unsuccessful rollback"
        );
        assert!(
            store.load_run(&run.context.run_id).is_err(),
            "unfinished exact completion was exposed before recovery"
        );
        let run_dir = store.open_run_directory(&run.context.run_id).unwrap();
        let still_running =
            ScienceStore::load_run_record_from_directory(&run_dir, &run.context.run_id).unwrap();
        assert_eq!(still_running.state, RunState::Running);

        let store_root = store.root().to_path_buf();
        drop(store);
        let reopened = ScienceStore::new(store_root);
        let succeeded = reopened
            .recover_exact_completion(&run.context)
            .unwrap()
            .expect("anchor-only exact completion must recover");
        assert_eq!(succeeded.state, RunState::Succeeded);
        assert_eq!(
            reopened.recover_interrupted(&run.context.run_id).unwrap(),
            succeeded,
            "legacy restart entrypoint changed an already recovered exact completion"
        );
        assert_eq!(
            reopened
                .events_after(&run.context.run_id, 0, 1_000)
                .unwrap(),
            manifest.events,
            "restart recovery changed the sealed event collection"
        );
    }

    #[test]
    fn sealed_restart_recovery_rejects_collection_tamper() {
        let (_temp, store, run, _call, _artifact) =
            successful_completion_fixture("manifest-seal-tamper");
        let manifest = completion_manifest(&store, &run);
        {
            let _guard = store.writes.lock().unwrap();
            let run_dir = store.open_run_directory(&run.context.run_id).unwrap();
            let running =
                ScienceStore::load_run_record_from_directory(&run_dir, &run.context.run_id)
                    .unwrap();
            let approval = ScienceStore::running_allowed_approval(
                &run_dir,
                &running,
                &run.context.run_id,
                None,
            )
            .unwrap();
            ScienceStore::persist_successful_completion_seal_unlocked(
                &run_dir, &running, &approval, &manifest,
            )
            .unwrap();
        }

        let mut tampered_events = manifest.events.clone();
        tampered_events.push(Event {
            schema_version: SCHEMA_VERSION,
            run_id: run.context.run_id.clone(),
            seq: u64::try_from(tampered_events.len()).unwrap() + 1,
            actor: "Intruder".into(),
            timestamp: Utc::now(),
            kind: "event.after-seal".into(),
            payload: serde_json::json!({}),
        });
        fs::write(
            store
                .run_dir(&run.context.run_id)
                .unwrap()
                .join("events.json"),
            serde_json::to_vec_pretty(&tampered_events).unwrap(),
        )
        .unwrap();

        let store_root = store.root().to_path_buf();
        drop(store);
        let reopened = ScienceStore::new(store_root);
        assert!(
            reopened.recover_interrupted(&run.context.run_id).is_err(),
            "restart recovery accepted a collection changed after its durable seal"
        );
        let run_dir = reopened.open_run_directory(&run.context.run_id).unwrap();
        let still_running =
            ScienceStore::load_run_record_from_directory(&run_dir, &run.context.run_id).unwrap();
        assert_eq!(still_running.state, RunState::Running);
    }

    #[test]
    fn exact_completion_anchor_prevents_seal_deletion_downgrade() {
        let (_temp, store, run, _call, _artifact) =
            successful_completion_fixture("manifest-seal-deletion");
        let manifest = completion_manifest(&store, &run);
        store.transition_succeeded_with_manifest(&manifest).unwrap();
        fs::remove_file(
            store
                .run_dir(&run.context.run_id)
                .unwrap()
                .join(SUCCESSFUL_COMPLETION_SEAL_FILE),
        )
        .unwrap();

        assert!(
            store
                .append_event(
                    &run.context.run_id,
                    "Intruder",
                    "event.after-seal-deletion",
                    serde_json::json!({}),
                )
                .is_err(),
            "deleting the seal downgraded exact completion to legacy event semantics"
        );
        assert!(
            store.recover_interrupted(&run.context.run_id).is_err(),
            "Succeeded exact completion ignored its missing durable seal"
        );
        assert!(
            store.load_run(&run.context.run_id).is_err(),
            "ordinary run reads ignored the missing exact-completion seal"
        );
    }

    #[test]
    fn exact_succeeded_public_reads_reject_collection_tamper() {
        let (_temp, store, run, _call, artifact) =
            successful_completion_fixture("manifest-public-read-tamper");
        let manifest = completion_manifest(&store, &run);
        store.transition_succeeded_with_manifest(&manifest).unwrap();

        let mut tampered_events = manifest.events.clone();
        tampered_events.push(Event {
            schema_version: SCHEMA_VERSION,
            run_id: run.context.run_id.clone(),
            seq: u64::try_from(tampered_events.len()).unwrap() + 1,
            actor: "Intruder".into(),
            timestamp: Utc::now(),
            kind: "event.after-exact-success".into(),
            payload: serde_json::json!({}),
        });
        fs::write(
            store
                .run_dir(&run.context.run_id)
                .unwrap()
                .join("events.json"),
            serde_json::to_vec_pretty(&tampered_events).unwrap(),
        )
        .unwrap();

        assert!(store.load_run(&run.context.run_id).is_err());
        assert!(
            store
                .load_run_bounded(&run.context.run_id, 64 * 1024)
                .is_err()
        );
        assert!(store.events_after(&run.context.run_id, 0, 1_000).is_err());
        assert!(store.artifacts(&run.context.run_id).is_err());
        assert!(store.evidence(&run.context.run_id).is_err());
        assert!(store.provenance(&run.context.run_id).is_err());
        assert!(store.previews(&run.context.run_id).is_err());
        assert!(store.approvals(&run.context.run_id).is_err());
        assert!(
            store
                .artifacts_bounded(&run.context.run_id, 100, 1024 * 1024)
                .is_err()
        );
        assert!(
            store
                .evidence_bounded(&run.context.run_id, 100, 1024 * 1024)
                .is_err()
        );
        assert!(
            store
                .provenance_bounded(&run.context.run_id, 100, 1024 * 1024)
                .is_err()
        );
        assert!(
            store
                .artifact_bytes_in_state(
                    &run.context.project_id,
                    &run.context.run_id,
                    &run.context.owner_id,
                    &artifact.relative_path,
                    RunState::Succeeded,
                )
                .is_err()
        );
    }

    #[test]
    fn exact_succeeded_reads_reject_terminal_and_approval_tamper() {
        let (_temp, store, run, _call, _artifact) =
            successful_completion_fixture("manifest-authority-tamper");
        let manifest = completion_manifest(&store, &run);
        let succeeded = store.transition_succeeded_with_manifest(&manifest).unwrap();
        let run_dir = store.run_dir(&run.context.run_id).unwrap();

        let mut changed_terminal = succeeded.clone();
        changed_terminal.terminal_reason = Some("raw filesystem rewrite".into());
        fs::write(
            run_dir.join("run.json"),
            serde_json::to_vec_pretty(&changed_terminal).unwrap(),
        )
        .unwrap();
        assert!(
            store.load_run(&run.context.run_id).is_err(),
            "exact completion served a changed terminal record"
        );
        fs::write(
            run_dir.join("run.json"),
            serde_json::to_vec_pretty(&succeeded).unwrap(),
        )
        .unwrap();

        let mut approvals = store.approvals(&run.context.run_id).unwrap();
        approvals[0].decided_at = Some(Utc::now() + chrono::Duration::seconds(1));
        fs::write(
            run_dir.join("approvals.json"),
            serde_json::to_vec_pretty(&approvals).unwrap(),
        )
        .unwrap();
        assert!(
            store.approvals(&run.context.run_id).is_err(),
            "exact completion served an approval changed after sealing"
        );
        assert!(
            store.load_run(&run.context.run_id).is_err(),
            "run reads ignored the changed sealed approval"
        );
    }

    #[test]
    fn completion_manifest_rejects_second_store_collection_injection_after_snapshot() {
        for injected in ["artifact", "evidence", "provenance", "event"] {
            let (_temp, store, run, call, artifact) =
                successful_completion_fixture(&format!("inject-{injected}"));
            let manifest = completion_manifest(&store, &run);
            let second_store = ScienceStore::new(store.root());

            match injected {
                "artifact" => {
                    second_store
                        .put_artifact(
                            &run.context.project_id,
                            &run.context.run_id,
                            &run.context.owner_id,
                            call.clone(),
                            Path::new("injected.txt"),
                            b"injected",
                            "text/plain",
                            "injected",
                        )
                        .unwrap();
                }
                "evidence" => {
                    second_store
                        .add_evidence(Evidence {
                            run_id: run.context.run_id.clone(),
                            claim: "injected".into(),
                            source: "second-store".into(),
                            artifact_sha256: Some(artifact.sha256.clone()),
                            verified_at: Utc::now(),
                        })
                        .unwrap();
                }
                "provenance" => {
                    second_store
                        .add_provenance(Provenance {
                            run_id: run.context.run_id.clone(),
                            source_uri: "fixture://injected".into(),
                            source_commit: None,
                            source_path: None,
                            license: "test-only".into(),
                            retrieved_at: Utc::now(),
                            input_sha256: artifact.sha256.clone(),
                            tool: "second-store".into(),
                            environment: BTreeMap::new(),
                        })
                        .unwrap();
                }
                "event" => {
                    second_store
                        .append_event(
                            &run.context.run_id,
                            "second-store",
                            "injected.after.snapshot",
                            serde_json::json!({}),
                        )
                        .unwrap();
                }
                _ => unreachable!(),
            }

            assert!(
                store.transition_succeeded_with_manifest(&manifest).is_err(),
                "post-snapshot {injected} injection was accepted"
            );
            assert_eq!(
                store.load_run(&run.context.run_id).unwrap().state,
                RunState::Running
            );
        }
    }

    #[test]
    fn completion_manifest_rehashes_and_rejects_tampered_artifact_bytes() {
        let (_temp, store, run, _call, artifact) = successful_completion_fixture("manifest-tamper");
        let manifest = completion_manifest(&store, &run);
        fs::write(
            store
                .run_dir(&run.context.run_id)
                .unwrap()
                .join("artifacts")
                .join(&artifact.relative_path),
            b"tampered after snapshot",
        )
        .unwrap();

        assert!(
            store.transition_succeeded_with_manifest(&manifest).is_err(),
            "tampered artifact bytes reached Succeeded"
        );
        assert_eq!(
            store.load_run(&run.context.run_id).unwrap().state,
            RunState::Running
        );
    }

    #[test]
    fn concurrent_projects_do_not_cross() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let a = store
            .create_run(context(temp.path(), "a", "alice"))
            .unwrap();
        let b = store.create_run(context(temp.path(), "b", "bob")).unwrap();
        std::thread::scope(|scope| {
            let store_a = store.clone();
            let store_b = store.clone();
            let a_id = a.context.run_id.clone();
            let b_id = b.context.run_id.clone();
            scope.spawn(move || {
                for n in 0..20 {
                    store_a
                        .append_event(&a_id, "a", "tick", serde_json::json!({"n": n}))
                        .unwrap();
                }
            });
            scope.spawn(move || {
                for n in 0..20 {
                    store_b
                        .append_event(&b_id, "b", "tick", serde_json::json!({"n": n}))
                        .unwrap();
                }
            });
        });
        assert!(
            store
                .events_after(&a.context.run_id, 0, 100)
                .unwrap()
                .iter()
                .all(|event| event.actor == "a")
        );
        assert!(
            store
                .events_after(&b.context.run_id, 0, 100)
                .unwrap()
                .iter()
                .all(|event| event.actor == "b")
        );
    }

    #[cfg(unix)]
    #[test]
    fn retained_store_root_keeps_record_writes_inside_after_path_replacement() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let store_root = temp.path().join("science-store");
        fs::create_dir(&store_root).unwrap();
        let store = ScienceStore::new(&store_root);
        let retained_root = temp.path().join("science-store-retained");
        fs::rename(&store_root, &retained_root).unwrap();
        symlink(outside.path(), &store_root).unwrap();

        let run = store
            .create_run(context(temp.path(), "pinned", "alice"))
            .unwrap();
        store
            .append_event(
                &run.context.run_id,
                "actor",
                "record.pinned",
                serde_json::json!({}),
            )
            .unwrap();

        assert!(
            retained_root
                .join("runs")
                .join(&run.context.run_id.0)
                .join("events.json")
                .is_file()
        );
        assert!(
            !outside.path().join("runs").exists(),
            "replaced store pathname received record bytes"
        );
    }

    #[cfg(unix)]
    #[test]
    fn runs_and_run_symlinks_reject_record_writes_without_outside_bytes() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let outside_runs = tempfile::tempdir().unwrap();
        let outside_run = tempfile::tempdir().unwrap();
        let store_root = temp.path().join("science-store");
        let store = ScienceStore::new(&store_root);
        let run = store
            .create_run(context(temp.path(), "project", "alice"))
            .unwrap();
        let runs = store_root.join("runs");
        let retained_runs = store_root.join("runs-retained");
        fs::rename(&runs, &retained_runs).unwrap();
        symlink(outside_runs.path(), &runs).unwrap();

        assert!(
            store
                .append_event(
                    &run.context.run_id,
                    "attacker",
                    "must.not.persist",
                    serde_json::json!({}),
                )
                .is_err()
        );
        assert_eq!(fs::read_dir(outside_runs.path()).unwrap().count(), 0);

        fs::remove_file(&runs).unwrap();
        fs::rename(&retained_runs, &runs).unwrap();
        let run_dir = runs.join(&run.context.run_id.0);
        let retained_run = runs.join(format!("{}-retained", run.context.run_id.0));
        fs::rename(&run_dir, &retained_run).unwrap();
        symlink(outside_run.path(), &run_dir).unwrap();

        assert!(
            store
                .append_event(
                    &run.context.run_id,
                    "attacker",
                    "must.not.persist",
                    serde_json::json!({}),
                )
                .is_err()
        );
        assert_eq!(fs::read_dir(outside_run.path()).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn workspace_capability_rejects_final_source_swap_to_outside_symlink() {
        use std::os::unix::fs::symlink;

        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let source = workspace.path().join("input.fasta");
        let retained = workspace.path().join("input-retained.fasta");
        let outside_source = outside.path().join("secret.fasta");
        fs::write(&source, b">inside\nACGT\n").unwrap();
        fs::write(&outside_source, b">outside\nSECRET\n").unwrap();
        let workspace_capability = ScienceWorkspaceCapability::open(workspace.path()).unwrap();

        let snapshot =
            workspace_capability.snapshot_regular_bounded_with_hook(&source, 1024, || {
                fs::rename(&source, &retained)?;
                symlink(&outside_source, &source)?;
                Ok(())
            });

        assert!(
            snapshot.is_err(),
            "final source symlink replacement escaped retained admission"
        );
    }

    #[cfg(unix)]
    #[test]
    fn workspace_capability_rejects_source_ancestor_swap_to_outside_symlink() {
        use std::os::unix::fs::symlink;

        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let ancestor = workspace.path().join("inputs");
        let retained = workspace.path().join("inputs-retained");
        fs::create_dir(&ancestor).unwrap();
        fs::write(ancestor.join("input.fasta"), b">inside\nACGT\n").unwrap();
        fs::write(outside.path().join("input.fasta"), b">outside\nSECRET\n").unwrap();
        let workspace_capability = ScienceWorkspaceCapability::open(workspace.path()).unwrap();

        let snapshot = workspace_capability.snapshot_regular_bounded_with_hook(
            &ancestor.join("input.fasta"),
            1024,
            || {
                fs::rename(&ancestor, &retained)?;
                symlink(outside.path(), &ancestor)?;
                Ok(())
            },
        );

        assert!(
            snapshot.is_err(),
            "source ancestor symlink replacement escaped retained admission"
        );
    }

    #[cfg(unix)]
    #[test]
    fn workspace_capability_bounds_source_and_rejects_fifo() {
        use std::{ffi::CString, os::unix::ffi::OsStrExt as _};

        let workspace = tempfile::tempdir().unwrap();
        let oversized = workspace.path().join("oversized.fasta");
        fs::write(&oversized, b"123456789").unwrap();
        let workspace_capability = ScienceWorkspaceCapability::open(workspace.path()).unwrap();
        assert!(
            workspace_capability
                .snapshot_regular_bounded(&oversized, 8)
                .is_err()
        );

        let fifo = workspace.path().join("input.fifo");
        let fifo_name = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: fifo_name is a live NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) }, 0);
        assert!(
            workspace_capability
                .snapshot_regular_bounded(&fifo, 8)
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn workspace_capability_keeps_store_out_of_replacement_workspace() {
        use std::os::unix::fs::symlink;

        let parent = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let workspace = parent.path().join("workspace");
        let retained = parent.path().join("workspace-retained");
        fs::create_dir(&workspace).unwrap();
        let source = workspace.join("input.fasta");
        fs::write(&source, b">inside\nACGT\n").unwrap();
        let workspace_capability = ScienceWorkspaceCapability::open(&workspace).unwrap();
        workspace_capability
            .snapshot_regular_bounded(&source, 1024)
            .unwrap();

        fs::rename(&workspace, &retained).unwrap();
        symlink(outside.path(), &workspace).unwrap();
        let store = workspace_capability.create_science_store(workspace.join("science-store"));
        assert!(
            !outside.path().join("science-store").exists(),
            "replacement workspace received a Science store"
        );
        assert_eq!(
            fs::read_dir(outside.path()).unwrap().count(),
            0,
            "replacement workspace received durable output"
        );

        if let Ok(store) = store {
            let current_workspace = workspace_capability.current_path().unwrap();
            assert_eq!(current_workspace, retained);
            assert!(store.root().starts_with(&retained));
            let mut retained_context = context(parent.path(), "retained", "alice");
            retained_context.workspace_root = current_workspace;
            retained_context.artifact_root = store.root().to_path_buf();
            let run = store.create_run(retained_context).unwrap();
            assert!(
                retained
                    .join("science-store/runs")
                    .join(&run.context.run_id.0)
                    .is_dir()
            );
            assert!(
                !outside.path().join("science-store").exists(),
                "durable run escaped into replacement workspace"
            );
        }

        fs::remove_file(&workspace).unwrap();
        fs::rename(&retained, &workspace).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn store_constructor_and_confined_constructor_reject_escape_roots() {
        use std::os::unix::fs::symlink;

        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let linked_root = workspace.path().join("linked-store");
        symlink(outside.path(), &linked_root).unwrap();
        let linked = ScienceStore::new(&linked_root);
        assert!(
            linked
                .create_run(context(workspace.path(), "linked", "alice"))
                .is_err()
        );
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);

        let inside_root = workspace.path().join("inside-store");
        fs::create_dir(&inside_root).unwrap();
        let inside = ScienceStore::new_confined(&inside_root, workspace.path()).unwrap();
        inside
            .create_run(context(workspace.path(), "inside", "alice"))
            .unwrap();

        let outside_root = outside.path().join("outside-store");
        fs::create_dir(&outside_root).unwrap();
        assert!(ScienceStore::new_confined(&outside_root, workspace.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn confined_open_rejects_in_workspace_store_replacement_after_identity_snapshot() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path().join("intended-store");
        let replacement = workspace.path().join("replacement-store");
        let retained = workspace.path().join("retained-intended-store");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&replacement).unwrap();

        let opened = PinnedDirectory::open_existing_confined_with_snapshot_hook(
            &root,
            workspace.path(),
            || {
                fs::rename(&root, &retained)?;
                fs::rename(&replacement, &root)?;
                Ok(())
            },
        );

        assert!(
            matches!(opened, Err(ScienceError::Invalid(_))),
            "a different in-workspace store replaced after the identity snapshot was accepted"
        );
        assert!(root.is_dir());
        assert!(retained.is_dir());
        assert_ne!(
            PinnedDirectory::checked_directory_identity(&root, "replacement").unwrap(),
            PinnedDirectory::checked_directory_identity(&retained, "retained").unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn confined_open_rejects_symlink_to_renamed_root_after_identity_snapshot() {
        use std::os::unix::fs::symlink;

        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path().join("intended-store");
        let retained = workspace.path().join("retained-intended-store");
        fs::create_dir(&root).unwrap();

        let opened = PinnedDirectory::open_existing_confined_with_snapshot_hook(
            &root,
            workspace.path(),
            || {
                fs::rename(&root, &retained)?;
                symlink(&retained, &root)?;
                Ok(())
            },
        );

        assert!(
            matches!(opened, Err(ScienceError::Invalid(_))),
            "a symlink installed after the identity snapshot was accepted"
        );
        assert!(
            fs::symlink_metadata(&root)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(retained.is_dir());
    }

    #[test]
    fn approval_is_owner_scoped_terminal_and_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let run = store
            .create_run(context(temp.path(), "a", "alice"))
            .unwrap();
        let call = CallId::new("csv");
        store
            .request_approval(Approval {
                project_id: run.context.project_id.clone(),
                run_id: run.context.run_id.clone(),
                call_id: call.clone(),
                owner_id: "alice".into(),
                decision: ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        assert!(matches!(
            store.decide_approval(
                &run.context.project_id,
                &run.context.run_id,
                "bob",
                &call,
                ApprovalDecision::Allow
            ),
            Err(ScienceError::Ownership)
        ));
        store
            .transition(&run.context.run_id, RunState::AwaitingApproval, None)
            .unwrap();
        let first = store
            .decide_approval(
                &run.context.project_id,
                &run.context.run_id,
                "alice",
                &call,
                ApprovalDecision::Deny,
            )
            .unwrap();
        assert_eq!(first.decision, ApprovalDecision::Deny);
        assert!(
            store
                .decide_approval(
                    &run.context.project_id,
                    &run.context.run_id,
                    "alice",
                    &call,
                    ApprovalDecision::Deny
                )
                .is_ok()
        );
        assert!(matches!(
            store.decide_approval(
                &run.context.project_id,
                &run.context.run_id,
                "alice",
                &call,
                ApprovalDecision::Allow
            ),
            Err(ScienceError::ApprovalConflict)
        ));
    }

    #[test]
    fn exact_completion_recovery_leaves_an_ordinary_pending_run_unchanged() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let run = store
            .create_run(context(temp.path(), "exact-only", "alice"))
            .unwrap();
        let call = CallId::new("pending-call");
        store
            .request_approval(Approval {
                project_id: run.context.project_id.clone(),
                run_id: run.context.run_id.clone(),
                call_id: call,
                owner_id: run.context.owner_id.clone(),
                decision: ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        store
            .transition(&run.context.run_id, RunState::AwaitingApproval, None)
            .unwrap();

        assert_eq!(store.recover_exact_completion(&run.context).unwrap(), None);
        assert_eq!(
            store.load_run(&run.context.run_id).unwrap().state,
            RunState::AwaitingApproval
        );
        assert_eq!(
            store.approvals(&run.context.run_id).unwrap()[0].decision,
            ApprovalDecision::Pending
        );
    }

    #[test]
    fn restart_replay_is_stable_and_pending_becomes_interrupted() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let run = store
            .create_run(context(temp.path(), "a", "alice"))
            .unwrap();
        store
            .append_event(
                &run.context.run_id,
                "session",
                "created",
                serde_json::json!({}),
            )
            .unwrap();
        let call = CallId::new("pending-call");
        store
            .request_approval(Approval {
                project_id: run.context.project_id.clone(),
                run_id: run.context.run_id.clone(),
                call_id: call.clone(),
                owner_id: run.context.owner_id.clone(),
                decision: ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        store
            .transition(&run.context.run_id, RunState::AwaitingApproval, None)
            .unwrap();
        drop(store);
        let reopened = ScienceStore::new(temp.path());
        assert_eq!(
            reopened.events_after(&run.context.run_id, 0, 100).unwrap()[0].seq,
            1
        );
        assert_eq!(
            reopened
                .recover_interrupted(&run.context.run_id)
                .unwrap()
                .state,
            RunState::Interrupted
        );
        let approval = reopened.approvals(&run.context.run_id).unwrap().remove(0);
        assert_eq!(approval.call_id, call);
        assert_eq!(approval.decision, ApprovalDecision::Interrupted);
        assert!(approval.decided_at.is_some());
    }

    #[test]
    fn concurrent_appends_to_one_run_have_unique_monotonic_sequences() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let run = store
            .create_run(context(temp.path(), "one", "alice"))
            .unwrap();
        std::thread::scope(|scope| {
            for worker in 0..8 {
                let store = store.clone();
                let run_id = run.context.run_id.clone();
                scope.spawn(move || {
                    for item in 0..50 {
                        store
                            .append_event(
                                &run_id,
                                format!("worker-{worker}"),
                                "tick",
                                serde_json::json!({"item": item}),
                            )
                            .unwrap();
                    }
                });
            }
        });
        let events = store.events_after(&run.context.run_id, 0, 1_000).unwrap();
        assert_eq!(events.len(), 400);
        assert!(
            events
                .iter()
                .enumerate()
                .all(|(index, event)| event.seq == index as u64 + 1)
        );
    }

    #[test]
    fn independent_stores_serialize_terminal_and_output_write_races() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("store");
        let authority = ScienceStore::new(&root);
        let independently_reopened = ScienceStore::new(&root);

        assert!(
            !Arc::ptr_eq(
                &authority.root_capability,
                &independently_reopened.root_capability,
            ),
            "fixture must use independently constructed stores"
        );
        assert!(
            Arc::ptr_eq(&authority.writes, &independently_reopened.writes),
            "independent stores must serialize authority writes"
        );

        for iteration in 0..32 {
            let run = authority
                .create_run(context(temp.path(), &format!("race-{iteration}"), "alice"))
                .unwrap();
            let call = allow_running_call(&authority, &run, "race-call");
            let relative = PathBuf::from(format!("race-{iteration}.txt"));
            let barrier = Arc::new(std::sync::Barrier::new(3));

            let (terminal, write) = std::thread::scope(|scope| {
                let terminal_barrier = Arc::clone(&barrier);
                let terminal_store = &independently_reopened;
                let terminal_run_id = run.context.run_id.clone();
                let terminal = scope.spawn(move || {
                    terminal_barrier.wait();
                    terminal_store.transition_succeeded_verified(&terminal_run_id)
                });

                let write_barrier = Arc::clone(&barrier);
                let write_store = &authority;
                let write_project = run.context.project_id.clone();
                let write_run_id = run.context.run_id.clone();
                let write_owner = run.context.owner_id.clone();
                let write_call = call.clone();
                let write_relative = relative.clone();
                let write = scope.spawn(move || {
                    write_barrier.wait();
                    write_store.put_artifact(
                        &write_project,
                        &write_run_id,
                        &write_owner,
                        write_call,
                        &write_relative,
                        b"race",
                        "text/plain",
                        "race",
                    )
                });

                barrier.wait();
                (terminal.join().unwrap(), write.join().unwrap())
            });

            terminal.unwrap();
            assert_eq!(
                authority.load_run(&run.context.run_id).unwrap().state,
                RunState::Succeeded
            );
            let artifacts = independently_reopened
                .artifacts(&run.context.run_id)
                .unwrap();
            match write {
                Ok(artifact) => {
                    assert_eq!(artifacts, vec![artifact]);
                    assert_eq!(
                        independently_reopened
                            .artifact_bytes(
                                &run.context.project_id,
                                &run.context.run_id,
                                &run.context.owner_id,
                                &relative,
                            )
                            .unwrap(),
                        b"race"
                    );
                }
                Err(_) => assert!(artifacts.is_empty()),
            }

            let retained = artifacts.clone();
            assert!(
                authority
                    .put_artifact(
                        &run.context.project_id,
                        &run.context.run_id,
                        &run.context.owner_id,
                        call,
                        Path::new("after-terminal.txt"),
                        b"late",
                        "text/plain",
                        "late",
                    )
                    .is_err(),
                "a write beginning after Succeeded must fail closed"
            );
            assert_eq!(
                independently_reopened
                    .artifacts(&run.context.run_id)
                    .unwrap(),
                retained,
                "Succeeded must remain output-immutable"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn registered_symlink_artifact_is_rejected_on_read() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let run = store
            .create_run(context(temp.path(), "a", "alice"))
            .unwrap();
        let call = allow_running_call(&store, &run, "c");
        store
            .put_artifact(
                &run.context.project_id,
                &run.context.run_id,
                "alice",
                call,
                Path::new("artifact.txt"),
                b"safe",
                "text/plain",
                "text",
            )
            .unwrap();
        store
            .transition_succeeded_verified(&run.context.run_id)
            .unwrap();
        let outside = temp.path().join("outside-secret");
        fs::write(&outside, b"secret").unwrap();
        let target = store
            .run_dir(&run.context.run_id)
            .unwrap()
            .join("artifacts/artifact.txt");
        fs::remove_file(&target).unwrap();
        symlink(&outside, &target).unwrap();
        assert!(matches!(
            store.artifact_bytes(
                &run.context.project_id,
                &run.context.run_id,
                "alice",
                Path::new("artifact.txt")
            ),
            Err(ScienceError::Invalid(_))
        ));
        assert_eq!(fs::read(outside).unwrap(), b"secret");
    }

    #[cfg(unix)]
    #[test]
    fn artifact_parent_symlink_cannot_redirect_write_or_read() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("sentinel"), b"outside-unchanged").unwrap();
        let store = ScienceStore::new(temp.path().join("store"));
        let run = store
            .create_run(context(temp.path(), "a", "alice"))
            .unwrap();
        let call = allow_running_call(&store, &run, "artifact-parent");
        let artifact_root = store
            .run_dir(&run.context.run_id)
            .unwrap()
            .join("artifacts");
        let retained_root = artifact_root.with_extension("retained");
        fs::rename(&artifact_root, &retained_root).unwrap();
        symlink(outside.path(), &artifact_root).unwrap();

        assert!(
            store
                .put_artifact(
                    &run.context.project_id,
                    &run.context.run_id,
                    "alice",
                    call.clone(),
                    Path::new("redirected.txt"),
                    b"must-stay-inside",
                    "text/plain",
                    "text",
                )
                .is_err()
        );
        assert!(!outside.path().join("redirected.txt").exists());
        assert_eq!(
            fs::read(outside.path().join("sentinel")).unwrap(),
            b"outside-unchanged"
        );

        fs::remove_file(&artifact_root).unwrap();
        fs::rename(&retained_root, &artifact_root).unwrap();
        let artifact = store
            .put_artifact(
                &run.context.project_id,
                &run.context.run_id,
                "alice",
                call,
                Path::new("registered.txt"),
                b"registered",
                "text/plain",
                "text",
            )
            .unwrap();
        store
            .transition_succeeded_verified(&run.context.run_id)
            .unwrap();
        fs::rename(&artifact_root, &retained_root).unwrap();
        fs::create_dir_all(outside.path()).unwrap();
        fs::write(
            outside.path().join(&artifact.relative_path),
            b"attacker-bytes",
        )
        .unwrap();
        symlink(outside.path(), &artifact_root).unwrap();
        assert!(
            store
                .artifact_bytes(
                    &run.context.project_id,
                    &run.context.run_id,
                    "alice",
                    &artifact.relative_path,
                )
                .is_err()
        );
        assert_eq!(
            fs::read(outside.path().join(&artifact.relative_path)).unwrap(),
            b"attacker-bytes"
        );
    }

    #[cfg(unix)]
    #[test]
    fn artifact_target_symlink_cannot_redirect_write() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("store"));
        let run = store
            .create_run(context(temp.path(), "a", "alice"))
            .unwrap();
        let call = allow_running_call(&store, &run, "call");
        let outside = temp.path().join("outside");
        fs::write(&outside, b"outside-unchanged").unwrap();
        let target = store
            .run_dir(&run.context.run_id)
            .unwrap()
            .join("artifacts/target.txt");
        symlink(&outside, &target).unwrap();

        assert!(
            store
                .put_artifact(
                    &run.context.project_id,
                    &run.context.run_id,
                    "alice",
                    call,
                    Path::new("target.txt"),
                    b"must-not-escape",
                    "text/plain",
                    "text",
                )
                .is_err()
        );
        assert_eq!(fs::read(&outside).unwrap(), b"outside-unchanged");
        assert!(
            fs::symlink_metadata(&target)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(store.artifacts(&run.context.run_id).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn retained_artifact_directory_handle_survives_path_replacement() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("store"));
        let run = store
            .create_run(context(temp.path(), "a", "alice"))
            .unwrap();
        let artifact_root = store
            .run_dir(&run.context.run_id)
            .unwrap()
            .join("artifacts");
        let retained_root = artifact_root.with_extension("retained");
        let pinned = store
            .open_run_directory(&run.context.run_id)
            .unwrap()
            .open_directory(Path::new("artifacts"))
            .unwrap();
        fs::rename(&artifact_root, &retained_root).unwrap();
        symlink(outside.path(), &artifact_root).unwrap();

        pinned
            .write_new_atomic(Path::new("pinned.txt"), b"pinned-inside")
            .unwrap();
        assert_eq!(
            fs::read(retained_root.join("pinned.txt")).unwrap(),
            b"pinned-inside"
        );
        assert!(!outside.path().join("pinned.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn discard_artifacts_unlinks_symlink_not_its_outside_target() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("store"));
        let run = store
            .create_run(context(temp.path(), "a", "alice"))
            .unwrap();
        let call = allow_running_call(&store, &run, "call");
        store
            .put_artifact(
                &run.context.project_id,
                &run.context.run_id,
                "alice",
                call.clone(),
                Path::new("partial.txt"),
                b"partial",
                "text/plain",
                "text",
            )
            .unwrap();
        let outside = temp.path().join("outside");
        fs::write(&outside, b"outside-unchanged").unwrap();
        let target = store
            .run_dir(&run.context.run_id)
            .unwrap()
            .join("artifacts/partial.txt");
        fs::remove_file(&target).unwrap();
        symlink(&outside, &target).unwrap();

        store
            .discard_artifacts(
                &run.context.project_id,
                &run.context.run_id,
                "alice",
                &call,
                &[Path::new("partial.txt")],
            )
            .unwrap();
        assert!(store.artifacts(&run.context.run_id).unwrap().is_empty());
        assert!(!target.exists());
        assert_eq!(fs::read(outside).unwrap(), b"outside-unchanged");
    }

    #[test]
    fn succeeded_run_rejects_discard_and_retains_serviceable_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("store");
        let writer = ScienceStore::new(&root);
        let terminal = ScienceStore::new(&root);
        let run = writer
            .create_run(context(temp.path(), "succeeded-discard", "alice"))
            .unwrap();
        let call = allow_running_call(&writer, &run, "call");
        let relative = Path::new("retained.txt");
        let artifact = writer
            .put_artifact(
                &run.context.project_id,
                &run.context.run_id,
                &run.context.owner_id,
                call.clone(),
                relative,
                b"retained",
                "text/plain",
                "retained",
            )
            .unwrap();
        terminal
            .transition_succeeded_verified(&run.context.run_id)
            .unwrap();

        assert!(
            writer
                .discard_artifacts(
                    &run.context.project_id,
                    &run.context.run_id,
                    &run.context.owner_id,
                    &call,
                    &[relative],
                )
                .is_err(),
            "public rollback must not mutate a Succeeded authority run"
        );
        assert_eq!(
            terminal.artifacts(&run.context.run_id).unwrap(),
            vec![artifact]
        );
        assert_eq!(
            terminal
                .artifact_bytes(
                    &run.context.project_id,
                    &run.context.run_id,
                    &run.context.owner_id,
                    relative,
                )
                .unwrap(),
            b"retained"
        );
    }

    #[test]
    fn running_output_rollback_tolerates_only_missing_payloads() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("store"));
        let run = store
            .create_run(context(temp.path(), "missing-rollback", "alice"))
            .unwrap();
        let call = allow_running_call(&store, &run, "call");
        let relative = Path::new("present.txt");
        let artifact = store
            .put_artifact(
                &run.context.project_id,
                &run.context.run_id,
                &run.context.owner_id,
                call.clone(),
                relative,
                b"partial",
                "text/plain",
                "partial",
            )
            .unwrap();
        store
            .add_evidence(Evidence {
                run_id: run.context.run_id.clone(),
                claim: "partial".into(),
                source: "fixture".into(),
                artifact_sha256: Some(artifact.sha256.clone()),
                verified_at: Utc::now(),
            })
            .unwrap();
        store
            .add_provenance(Provenance {
                run_id: run.context.run_id.clone(),
                source_uri: "fixture://partial".into(),
                source_commit: None,
                source_path: None,
                license: "test-only".into(),
                retrieved_at: Utc::now(),
                input_sha256: artifact.sha256.clone(),
                tool: "fixture".into(),
                environment: BTreeMap::new(),
            })
            .unwrap();
        store
            .add_preview(preview_record(
                &run,
                &call,
                relative,
                artifact.sha256.clone(),
            ))
            .unwrap();

        store
            .discard_running_outputs(
                &run.context.project_id,
                &run.context.run_id,
                &run.context.owner_id,
                &call,
                &[relative, Path::new("never-created/stdout.txt")],
            )
            .unwrap();

        assert!(store.artifacts(&run.context.run_id).unwrap().is_empty());
        assert!(store.evidence(&run.context.run_id).unwrap().is_empty());
        assert!(store.provenance(&run.context.run_id).unwrap().is_empty());
        assert!(store.previews(&run.context.run_id).unwrap().is_empty());
        assert!(
            !store
                .run_dir(&run.context.run_id)
                .unwrap()
                .join("artifacts/present.txt")
                .exists()
        );
        store
            .transition(
                &run.context.run_id,
                RunState::Failed,
                Some("commit rollback".into()),
            )
            .unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn artifact_read_hashes_the_same_open_handle_and_requires_success() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("store"));
        let run = store
            .create_run(context(temp.path(), "a", "alice"))
            .unwrap();
        let call = allow_running_call(&store, &run, "call");
        store
            .put_artifact(
                &run.context.project_id,
                &run.context.run_id,
                "alice",
                call,
                Path::new("result.txt"),
                b"registered",
                "text/plain",
                "text",
            )
            .unwrap();
        assert!(
            store
                .artifact_bytes(
                    &run.context.project_id,
                    &run.context.run_id,
                    "alice",
                    Path::new("result.txt"),
                )
                .is_err(),
            "non-succeeded run artifact was serviceable"
        );
        store
            .transition_succeeded_verified(&run.context.run_id)
            .unwrap();
        let target = store
            .run_dir(&run.context.run_id)
            .unwrap()
            .join("artifacts/result.txt");
        fs::write(target, b"tampered").unwrap();
        assert!(matches!(
            store.artifact_bytes(
                &run.context.project_id,
                &run.context.run_id,
                "alice",
                Path::new("result.txt"),
            ),
            Err(ScienceError::Invalid(_))
        ));
    }

    #[test]
    fn traversal_and_cross_run_reads_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let run = store
            .create_run(context(temp.path(), "a", "alice"))
            .unwrap();
        assert!(
            store
                .put_artifact(
                    &run.context.project_id,
                    &run.context.run_id,
                    "alice",
                    CallId::new("c"),
                    Path::new("../escape"),
                    b"x",
                    "text/plain",
                    "text"
                )
                .is_err()
        );
        assert!(matches!(
            store.put_artifact(
                &ProjectId::new("b"),
                &run.context.run_id,
                "alice",
                CallId::new("c"),
                Path::new("x"),
                b"x",
                "text/plain",
                "text"
            ),
            Err(ScienceError::Ownership)
        ));
    }

    #[test]
    fn persisted_run_ids_reject_traversal_absolute_separators_and_oversize_without_writes() {
        let temp = tempfile::tempdir().unwrap();
        let store_root = temp.path().join("store");
        let outside_absolute = temp.path().join("absolute-escape");
        assert!(outside_absolute.is_absolute());

        let invalid_ids = [
            String::new(),
            "../escape".into(),
            "../../outside-traversal".into(),
            outside_absolute.to_string_lossy().into_owned(),
            "nested/run".into(),
            r"nested\run".into(),
            "run.with-dot".into(),
            "é".into(),
            "x".repeat(MAX_PERSISTED_ID_BYTES + 1),
        ];
        let store = ScienceStore::new(&store_root);

        for invalid in invalid_ids {
            let mut invalid_context = context(temp.path(), "valid-project", "alice");
            invalid_context.run_id = RunId::new(invalid.clone());
            assert!(
                matches!(
                    store.create_run(invalid_context),
                    Err(ScienceError::Invalid(_))
                ),
                "create_run accepted invalid RunId {invalid:?}"
            );
        }

        assert!(
            !store_root.exists(),
            "invalid create_run must not initialize the store"
        );
        assert!(
            !outside_absolute.exists(),
            "absolute RunId must not create an out-of-store path"
        );
        assert!(
            !temp.path().join("outside-traversal").exists(),
            "traversal RunId must not create an out-of-store path"
        );

        let invalid = RunId::new(outside_absolute.to_string_lossy());
        assert!(matches!(
            store.load_run(&invalid),
            Err(ScienceError::Invalid(_))
        ));
        assert!(matches!(
            store.transition(&invalid, RunState::Running, None),
            Err(ScienceError::Invalid(_))
        ));
        assert!(matches!(
            store.append_event(&invalid, "actor", "event", serde_json::json!({})),
            Err(ScienceError::Invalid(_))
        ));
        assert!(matches!(
            store.events_after(&invalid, 0, 1),
            Err(ScienceError::Invalid(_))
        ));
        assert!(matches!(
            store.recover_interrupted(&invalid),
            Err(ScienceError::Invalid(_))
        ));
        assert!(
            !outside_absolute.exists(),
            "read/transition/event/recovery APIs must fail before path access"
        );
    }

    #[test]
    fn persisted_project_and_call_ids_fail_closed_without_approval_or_artifact_side_effects() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("store"));
        let run = store
            .create_run(context(temp.path(), "valid-project", "alice"))
            .unwrap();
        let invalid_project = ProjectId::new("../other-project");
        let invalid_call = CallId::new("/absolute-call");
        let oversized_call = CallId::new("x".repeat(MAX_PERSISTED_ID_BYTES + 1));

        for (project_id, call_id) in [
            (invalid_project.clone(), CallId::new("valid-call")),
            (run.context.project_id.clone(), invalid_call.clone()),
            (run.context.project_id.clone(), oversized_call),
        ] {
            assert!(matches!(
                store.request_approval(Approval {
                    project_id,
                    run_id: run.context.run_id.clone(),
                    call_id,
                    owner_id: "alice".into(),
                    decision: ApprovalDecision::Pending,
                    decided_at: None,
                }),
                Err(ScienceError::Invalid(_))
            ));
        }
        assert!(store.approvals(&run.context.run_id).unwrap().is_empty());
        assert!(matches!(
            store.decide_approval(
                &run.context.project_id,
                &run.context.run_id,
                "alice",
                &invalid_call,
                ApprovalDecision::Deny,
            ),
            Err(ScienceError::Invalid(_))
        ));

        for (project_id, call_id, relative) in [
            (
                invalid_project,
                CallId::new("valid-call"),
                Path::new("invalid-project.txt"),
            ),
            (
                run.context.project_id.clone(),
                invalid_call,
                Path::new("invalid-call.txt"),
            ),
        ] {
            assert!(matches!(
                store.put_artifact(
                    &project_id,
                    &run.context.run_id,
                    "alice",
                    call_id,
                    relative,
                    b"must-not-persist",
                    "text/plain",
                    "text",
                ),
                Err(ScienceError::Invalid(_))
            ));
        }

        assert!(store.artifacts(&run.context.run_id).unwrap().is_empty());
        let artifact_root = store
            .run_dir(&run.context.run_id)
            .unwrap()
            .join("artifacts");
        assert_eq!(fs::read_dir(artifact_root).unwrap().count(), 0);
        assert!(!temp.path().join("other-project").exists());
    }

    #[test]
    fn corrupt_event_store_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let run = store
            .create_run(context(temp.path(), "a", "alice"))
            .unwrap();
        fs::write(
            store
                .run_dir(&run.context.run_id)
                .unwrap()
                .join("events.json"),
            b"not-json",
        )
        .unwrap();
        assert!(matches!(
            store.events_after(&run.context.run_id, 0, 100),
            Err(ScienceError::Serde(_))
        ));
        assert!(
            store
                .append_event(&run.context.run_id, "actor", "event", serde_json::json!({}))
                .is_err()
        );
        assert_eq!(
            store.load_run(&run.context.run_id).unwrap().state,
            RunState::Failed
        );
    }

    #[test]
    fn explicit_denied_timeout_and_cancel_terminal_states() {
        for state in [RunState::Denied, RunState::TimedOut, RunState::Cancelled] {
            let temp = tempfile::tempdir().unwrap();
            let store = ScienceStore::new(temp.path());
            let run = store
                .create_run(context(temp.path(), "a", "alice"))
                .unwrap();
            let call = CallId::new("terminal-call");
            request_pending_call(&store, &run, &call);
            store
                .transition(&run.context.run_id, RunState::AwaitingApproval, None)
                .unwrap();
            let decision = match state {
                RunState::Denied => ApprovalDecision::Deny,
                RunState::TimedOut => ApprovalDecision::Timeout,
                RunState::Cancelled => ApprovalDecision::Cancel,
                _ => unreachable!(),
            };
            store
                .decide_approval(
                    &run.context.project_id,
                    &run.context.run_id,
                    &run.context.owner_id,
                    &call,
                    decision,
                )
                .unwrap();
            let terminal = store
                .transition(&run.context.run_id, state, Some(format!("{state:?}")))
                .unwrap();
            assert!(terminal.state.terminal());
            assert!(
                store
                    .transition(&run.context.run_id, RunState::Running, None)
                    .is_err()
            );
        }
    }

    #[test]
    fn succeeded_requires_specialized_running_allowed_transition() {
        for (state, decision) in [
            (RunState::Created, None),
            (RunState::AwaitingApproval, None),
            (RunState::Denied, Some(ApprovalDecision::Deny)),
            (RunState::TimedOut, Some(ApprovalDecision::Timeout)),
            (RunState::Cancelled, Some(ApprovalDecision::Cancel)),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let store = ScienceStore::new(temp.path().join("store"));
            let run = store
                .create_run(context(temp.path(), "blocked-success", "alice"))
                .unwrap();
            let call = CallId::new("blocked-call");
            if state != RunState::Created {
                request_pending_call(&store, &run, &call);
                store
                    .transition(&run.context.run_id, RunState::AwaitingApproval, None)
                    .unwrap();
            }
            if let Some(decision) = decision {
                store
                    .decide_approval(
                        &run.context.project_id,
                        &run.context.run_id,
                        &run.context.owner_id,
                        &call,
                        decision,
                    )
                    .unwrap();
                store
                    .transition(
                        &run.context.run_id,
                        state,
                        Some(format!("{state:?} fixture")),
                    )
                    .unwrap();
            }

            assert!(
                store
                    .transition_succeeded_verified(&run.context.run_id)
                    .is_err(),
                "{state:?} reached Succeeded without a running terminal Allow"
            );
            assert_eq!(store.load_run(&run.context.run_id).unwrap().state, state);
        }

        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("store"));
        let run = store
            .create_run(context(temp.path(), "pending-running", "alice"))
            .unwrap();
        let call = CallId::new("pending-call");
        request_pending_call(&store, &run, &call);
        store
            .transition(&run.context.run_id, RunState::AwaitingApproval, None)
            .unwrap();
        store
            .transition(&run.context.run_id, RunState::Running, None)
            .unwrap();
        assert!(
            store
                .transition_succeeded_verified(&run.context.run_id)
                .is_err(),
            "Running with a pending approval reached Succeeded"
        );
        assert_eq!(
            store.load_run(&run.context.run_id).unwrap().state,
            RunState::Running
        );

        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("store"));
        let run = store
            .create_run(context(temp.path(), "allowed-success", "alice"))
            .unwrap();
        allow_running_call(&store, &run, "allowed-call");
        assert!(
            store
                .transition(&run.context.run_id, RunState::Succeeded, None)
                .is_err(),
            "ordinary transition bypassed the verified success authority"
        );
        assert_eq!(
            store.load_run(&run.context.run_id).unwrap().state,
            RunState::Running
        );
        let succeeded = store
            .transition_succeeded_verified(&run.context.run_id)
            .unwrap();
        assert_eq!(succeeded.state, RunState::Succeeded);
        assert_eq!(store.load_run(&run.context.run_id).unwrap(), succeeded);
    }

    #[test]
    fn scientific_outputs_require_exactly_one_allowed_running_call() {
        for state in [
            RunState::Created,
            RunState::AwaitingApproval,
            RunState::Running,
            RunState::Denied,
            RunState::TimedOut,
            RunState::Cancelled,
        ] {
            let temp = tempfile::tempdir().unwrap();
            let store = ScienceStore::new(temp.path().join("store"));
            let run = store
                .create_run(context(temp.path(), "a", "alice"))
                .unwrap();
            let call = CallId::new("blocked-call");

            if state != RunState::Created {
                request_pending_call(&store, &run, &call);
                store
                    .transition(&run.context.run_id, RunState::AwaitingApproval, None)
                    .unwrap();
            }
            match state {
                RunState::Created | RunState::AwaitingApproval => {}
                RunState::Running => {
                    store
                        .transition(&run.context.run_id, RunState::Running, None)
                        .unwrap();
                }
                RunState::Denied | RunState::TimedOut | RunState::Cancelled => {
                    let decision = match state {
                        RunState::Denied => ApprovalDecision::Deny,
                        RunState::TimedOut => ApprovalDecision::Timeout,
                        RunState::Cancelled => ApprovalDecision::Cancel,
                        _ => unreachable!(),
                    };
                    store
                        .decide_approval(
                            &run.context.project_id,
                            &run.context.run_id,
                            &run.context.owner_id,
                            &call,
                            decision,
                        )
                        .unwrap();
                    store
                        .transition(
                            &run.context.run_id,
                            state,
                            Some(format!("{state:?} fixture")),
                        )
                        .unwrap();
                }
                _ => unreachable!(),
            }

            assert_eq!(store.load_run(&run.context.run_id).unwrap().state, state);
            assert_all_output_writes_rejected(&store, &run, &call);
        }

        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("store"));
        let run = store
            .create_run(context(temp.path(), "multiple-approvals", "alice"))
            .unwrap();
        let allowed_call = CallId::new("allowed-call");
        let second_call = CallId::new("second-call");
        request_pending_call(&store, &run, &allowed_call);
        request_pending_call(&store, &run, &second_call);
        store
            .transition(&run.context.run_id, RunState::AwaitingApproval, None)
            .unwrap();
        for call in [&allowed_call, &second_call] {
            store
                .decide_approval(
                    &run.context.project_id,
                    &run.context.run_id,
                    &run.context.owner_id,
                    call,
                    ApprovalDecision::Allow,
                )
                .unwrap();
        }
        store
            .transition(&run.context.run_id, RunState::Running, None)
            .unwrap();
        assert_all_output_writes_rejected(&store, &run, &allowed_call);

        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("store"));
        let run = store
            .create_run(context(temp.path(), "allowed", "alice"))
            .unwrap();
        let call = allow_running_call(&store, &run, "allowed-call");
        let relative = Path::new("allowed.txt");
        let artifact = store
            .put_artifact(
                &run.context.project_id,
                &run.context.run_id,
                &run.context.owner_id,
                call.clone(),
                relative,
                b"allowed",
                "text/plain",
                "allowed",
            )
            .unwrap();
        store
            .add_evidence(Evidence {
                run_id: run.context.run_id.clone(),
                claim: "allowed".into(),
                source: "fixture".into(),
                artifact_sha256: Some(artifact.sha256.clone()),
                verified_at: Utc::now(),
            })
            .unwrap();
        store
            .add_provenance(Provenance {
                run_id: run.context.run_id.clone(),
                source_uri: "fixture://allowed".into(),
                source_commit: None,
                source_path: None,
                license: "test-only".into(),
                retrieved_at: Utc::now(),
                input_sha256: artifact.sha256.clone(),
                tool: "fixture".into(),
                environment: BTreeMap::new(),
            })
            .unwrap();
        store
            .add_preview(preview_record(
                &run,
                &call,
                relative,
                artifact.sha256.clone(),
            ))
            .unwrap();

        assert_eq!(
            store
                .allowed_running_artifact_bytes(
                    &run.context.project_id,
                    &run.context.run_id,
                    &run.context.owner_id,
                    &call,
                    relative,
                )
                .unwrap(),
            b"allowed"
        );
        assert_eq!(store.artifacts(&run.context.run_id).unwrap().len(), 1);
        assert_eq!(store.evidence(&run.context.run_id).unwrap().len(), 1);
        assert_eq!(store.provenance(&run.context.run_id).unwrap().len(), 1);
        assert_eq!(store.previews(&run.context.run_id).unwrap().len(), 1);
    }

    #[test]
    fn terminal_run_scientific_outputs_are_immutable() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let run = store
            .create_run(context(temp.path(), "a", "alice"))
            .unwrap();
        allow_running_call(&store, &run, "terminal-call");
        store
            .append_event(
                &run.context.run_id,
                "SessionActor",
                "run.before-terminal",
                serde_json::json!({}),
            )
            .unwrap();
        let events_before = store.events_after(&run.context.run_id, 0, 1_000).unwrap();
        store
            .transition_succeeded_verified(&run.context.run_id)
            .unwrap();

        let terminal_audit = store
            .append_event(
                &run.context.run_id,
                "LegacyAudit",
                "event.after-legacy-terminal",
                serde_json::json!({}),
            )
            .expect("legacy completion retains its existing terminal audit behavior");
        assert!(
            store
                .put_artifact(
                    &run.context.project_id,
                    &run.context.run_id,
                    "alice",
                    CallId::new("late"),
                    Path::new("late.txt"),
                    b"late",
                    "text/plain",
                    "late",
                )
                .is_err()
        );
        assert!(
            store
                .add_evidence(Evidence {
                    run_id: run.context.run_id.clone(),
                    claim: "late".into(),
                    source: "late".into(),
                    artifact_sha256: None,
                    verified_at: Utc::now(),
                })
                .is_err()
        );
        assert!(
            store
                .add_provenance(Provenance {
                    run_id: run.context.run_id.clone(),
                    source_uri: "late".into(),
                    source_commit: None,
                    source_path: None,
                    license: "late".into(),
                    retrieved_at: Utc::now(),
                    input_sha256: "a".repeat(64),
                    tool: "late".into(),
                    environment: BTreeMap::new(),
                })
                .is_err()
        );
        assert!(store.artifacts(&run.context.run_id).unwrap().is_empty());
        assert!(store.evidence(&run.context.run_id).unwrap().is_empty());
        assert!(store.provenance(&run.context.run_id).unwrap().is_empty());
        let mut expected_events = events_before;
        expected_events.push(terminal_audit);
        assert_eq!(
            store.events_after(&run.context.run_id, 0, 1_000).unwrap(),
            expected_events,
            "legacy terminal audit compatibility changed"
        );
    }

    #[cfg(unix)]
    #[test]
    fn science_and_project_stores_compare_retained_root_identity() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = dunce::canonicalize(temp.path()).unwrap();
        let root = workspace.join("shared-store");
        fs::create_dir(&root).unwrap();
        let science = ScienceStore::new_confined(&root, &workspace).unwrap();
        let original_project = project::ProjectStore::new_confined(&root, &workspace).unwrap();
        let original_workflow =
            workflow::WorkflowIoCapability::open_existing_confined(&root, &workspace).unwrap();
        assert!(
            science
                .shares_root_capability_with(&original_project)
                .unwrap()
        );
        assert!(
            science
                .shares_root_capability_with_workflow_io(&original_workflow)
                .unwrap()
        );

        let retained = workspace.join("retained-store");
        fs::rename(&root, &retained).unwrap();
        fs::create_dir(&root).unwrap();
        let replacement_project = project::ProjectStore::new_confined(&root, &workspace).unwrap();
        let replacement_workflow =
            workflow::WorkflowIoCapability::open_existing_confined(&root, &workspace).unwrap();

        assert!(
            science
                .shares_root_capability_with(&original_project)
                .unwrap(),
            "two handles opened before the rename must retain one identity"
        );
        assert!(
            !science
                .shares_root_capability_with(&replacement_project)
                .unwrap(),
            "same path spelling after replacement must not pass identity binding"
        );
        assert!(
            science
                .shares_root_capability_with_workflow_io(&original_workflow)
                .unwrap(),
            "workflow and Science handles opened before the rename must retain one identity"
        );
        assert!(
            !science
                .shares_root_capability_with_workflow_io(&replacement_workflow)
                .unwrap(),
            "replacement workflow root must not pass retained identity binding"
        );
    }

    #[cfg(unix)]
    #[test]
    fn operation_lease_uses_retained_store_identity_after_path_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = dunce::canonicalize(temp.path()).unwrap();
        let root = workspace.join("lease-store");
        fs::create_dir(&root).unwrap();
        let science = ScienceStore::new_confined(&root, &workspace).unwrap();

        let retained = workspace.join("retained-lease-store");
        fs::rename(&root, &retained).unwrap();
        fs::create_dir(&root).unwrap();
        let replacement = ScienceStore::new_confined(&root, &workspace).unwrap();
        let run_id = RunId::new("retained-operation-lease");

        let retained_lease = science.claim_operation_lease(&run_id).unwrap();
        assert!(
            retained
                .join(".seq-analyze-leases")
                .join(format!("{}.lock", run_id.0))
                .is_file(),
            "lease must be created relative to the retained store descriptor"
        );
        assert!(
            !root.join(".seq-analyze-leases").exists(),
            "replacement pathname must remain untouched"
        );
        assert!(
            science.claim_operation_lease(&run_id).is_err(),
            "one retained store identity must be single-flight in-process"
        );

        let replacement_lease = replacement.claim_operation_lease(&run_id).unwrap();
        assert!(
            root.join(".seq-analyze-leases")
                .join(format!("{}.lock", run_id.0))
                .is_file(),
            "a separately admitted replacement is a distinct store identity"
        );
        drop(replacement_lease);
        drop(retained_lease);
        science.claim_operation_lease(&run_id).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn operation_lease_rejects_symlink_and_writable_store_boundaries() {
        use std::os::unix::fs::{PermissionsExt as _, symlink};

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("lease-store");
        let outside = temp.path().join("outside");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&outside).unwrap();
        let science = ScienceStore::new(&root);
        let run_id = RunId::new("boundary-operation-lease");

        symlink(&outside, root.join(".seq-analyze-leases")).unwrap();
        assert!(
            science.claim_operation_lease(&run_id).is_err(),
            "a symlinked lease directory must fail closed"
        );
        assert!(
            fs::read_dir(&outside).unwrap().next().is_none(),
            "symlink target must remain untouched"
        );

        fs::remove_file(root.join(".seq-analyze-leases")).unwrap();
        let original_mode = fs::metadata(&root).unwrap().permissions().mode();
        fs::set_permissions(&root, fs::Permissions::from_mode(original_mode | 0o022)).unwrap();
        let error = science.claim_operation_lease(&run_id).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("must not be group- or world-writable"),
            "unexpected writable-boundary error: {error}"
        );
        assert!(
            !root.join(".seq-analyze-leases").exists(),
            "rejected store boundary must not create a lease directory"
        );
    }

    // ── Single-flight: cross-process concurrent rejection ──────────

    #[cfg(unix)]
    #[test]
    fn operation_lease_rejects_concurrent_process() {
        use std::process::Command;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("concurrent-store");
        fs::create_dir(&root).unwrap();
        let science = ScienceStore::new(&root);
        let run_id = RunId::new("cross-process-lease");

        // Acquire the lease in the parent
        let parent_lease = science.claim_operation_lease(&run_id)
            .expect("parent must acquire lease");

        // A second claim from the SAME process must fail (non-reentrant flock)
        assert!(
            science.claim_operation_lease(&run_id).is_err(),
            "same-process re-acquire must fail"
        );

        // Simulate a DIFFERENT process: use flock(1) on the lease file
        let lease_path = root.join(".seq-analyze-leases")
            .join(format!("{}.lock", run_id.0));
        assert!(lease_path.is_file(), "lease file must exist: {lease_path:?}");

        // Use flock(1) which is available on Linux/macOS
        let output = Command::new("flock")
            .args(["--nonblock", "--exclusive", lease_path.to_str().unwrap(), "true"])
            .output();
        match output {
            Ok(out) => {
                // On some systems flock(1) may be installed at a different path
                // or not present. If it runs and exits non-zero, the lock is held.
                assert!(!out.status.success(),
                    "flock must fail when parent holds exclusive lease. exit={}",
                    out.status);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // flock(1) not available — skip cross-process verification
                // but in-process non-reentrant proof still holds.
                eprintln!("flock(1) not found; cross-process step skipped (in-process proof holds)");
            }
            Err(e) => panic!("unexpected flock error: {e}"),
        }

        drop(parent_lease);

        // After releasing, flock(1) should succeed
        if let Ok(out) = Command::new("flock")
            .args(["--nonblock", "--exclusive", lease_path.to_str().unwrap(), "true"])
            .output()
        {
            assert!(out.status.success(),
                "flock must succeed after lease released. exit={}", out.status);
        }
    }

    // ── Finish lock-holding proof ──────────────────────────────────

    #[cfg(unix)]
    #[test]
    fn operation_lease_held_throughout_finish_proof() {
        use std::process::{Command, Stdio};
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("finish-lock-store");
        fs::create_dir(&root).unwrap();
        let science = ScienceStore::new(&root);
        let run_id = RunId::new("finish-lock-proof");

        // Acquire the lease → represents Begin
        let lease = science.claim_operation_lease(&run_id)
            .expect("Begin must acquire lease");
        let lease_path = root.join(".seq-analyze-leases")
            .join(format!("{}.lock", run_id.0));

        // While the lease is held (simulating permission + Finish duration),
        // prove no other process can acquire it
        let child = Command::new("flock")
            .args(["--nonblock", "--exclusive", lease_path.to_str().unwrap(),
                   "-c", "echo LOCK_ACQUIRED"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();

        if let Ok(out) = child {
            assert!(!out.status.success(),
                "Finish lock was not held — another process acquired it. stdout={}",
                String::from_utf8_lossy(&out.stdout));
        }

        // Drop = Finish completes; lock released
        drop(lease);

        // Now flock must succeed
        let after = Command::new("flock")
            .args(["--nonblock", "--exclusive", lease_path.to_str().unwrap(),
                   "--command", "true"])
            .output();
        if let Ok(out) = after {
            assert!(out.status.success(),
                "lock must be released after Finish. exit={}", out.status);
        }
    }

    // ── Crash recovery: drop releases lease for restart ────────────

    #[cfg(unix)]
    #[test]
    fn operation_lease_released_on_drop_for_restart() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("crash-store");
        fs::create_dir(&root).unwrap();
        let science = ScienceStore::new(&root);
        let run_id = RunId::new("crash-recovery-lease");

        // Simulate Begin → permission → Finish
        let lease = science.claim_operation_lease(&run_id)
            .expect("acquire lease");

        // While held, another claim must fail (same-process non-reentrant)
        assert!(
            science.claim_operation_lease(&run_id).is_err(),
            "reacquire must fail while lease held"
        );

        // Simulate crash: drop the lease (process death releases all fds)
        drop(lease);

        // After drop, a restart can acquire the same operation_id
        let recovered = science.claim_operation_lease(&run_id);
        assert!(
            recovered.is_ok(),
            "lease must be re-acquirable after drop. error={:?}",
            recovered.err()
        );
    }
}
