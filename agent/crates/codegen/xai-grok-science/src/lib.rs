//! Durable Lumen Science kernel. Seam contract: S1, S2, S4.
//!
//! This crate owns records, never execution authority. Product execution must
//! enter through `xai-grok-shell::SessionActor` before calling this crate.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};
use uuid::Uuid;

pub mod api;
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
pub mod project;
pub mod release;
pub mod remote;
pub mod review;
pub mod seqbench;
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    pub schema_version: u32,
    pub context: RunContext,
    pub state: RunState,
    pub terminal_reason: Option<String>,
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

#[derive(Debug, Clone)]
pub struct ScienceStore {
    root: PathBuf,
    root_capability: Arc<Mutex<StoreRootCapability>>,
    writes: Arc<Mutex<()>>,
}

#[derive(Debug)]
enum StoreRootCapability {
    Pending,
    Pinned(PinnedDirectory),
    Unavailable(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoreRootIdentity {
    #[cfg(unix)]
    Unix { device: u64, inode: u64 },
    #[cfg(windows)]
    Windows { volume: u32, index: u64 },
}

impl ScienceStore {
    const MAX_DOSSIER_REGISTRY_BYTES: u64 = 8 * 1024 * 1024;
    const MAX_DOSSIER_RUN_RECORD_BYTES: u64 = 1024 * 1024;

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
            writes: Arc::new(Mutex::new(())),
        }
    }

    /// Open and retain the store root first, then prove the opened directory's
    /// handle-resolved location is inside the canonical workspace boundary.
    /// Product adapters use this constructor after provisioning the root so an
    /// ancestor swap between pathname validation and store construction cannot
    /// redirect later record writes.
    pub fn new_confined(
        root: impl Into<PathBuf>,
        workspace_root: impl AsRef<Path>,
    ) -> Result<Self> {
        let root = root.into();
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
        Ok(Self {
            root,
            root_capability: Arc::new(Mutex::new(StoreRootCapability::Pinned(directory))),
            writes: Arc::new(Mutex::new(())),
        })
    }

    /// Durable root owned by this store. Product adapters use this only to
    /// prove the store they hand to a SessionActor is the same confined root
    /// recorded in the run context; callers still cannot construct run paths.
    pub fn root(&self) -> &Path {
        &self.root
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

    pub fn create_run(&self, context: RunContext) -> Result<RunRecord> {
        validate_context(&context)?;
        let record = RunRecord {
            schema_version: SCHEMA_VERSION,
            context,
            state: RunState::Created,
            terminal_reason: None,
        };
        let root = self.root_directory()?;
        let runs = root.create_directories(Path::new("runs"))?;
        let dir = runs.create_directory_new(Path::new(&record.context.run_id.0))?;
        dir.create_directories(Path::new("artifacts"))?;
        dir.replace_json_atomic(Path::new("run.json"), &record)?;
        dir.replace_json_atomic(Path::new("events.json"), &Vec::<Event>::new())?;
        dir.replace_json_atomic(Path::new("artifacts.json"), &Vec::<Artifact>::new())?;
        dir.replace_json_atomic(Path::new("evidence.json"), &Vec::<Evidence>::new())?;
        dir.replace_json_atomic(Path::new("provenance.json"), &Vec::<Provenance>::new())?;
        dir.replace_json_atomic(Path::new("approvals.json"), &Vec::<Approval>::new())?;
        dir.replace_json_atomic(
            Path::new("previews.json"),
            &Vec::<preview::PreviewRecord>::new(),
        )?;
        Ok(record)
    }

    pub fn load_run(&self, run_id: &RunId) -> Result<RunRecord> {
        let run: RunRecord = self
            .open_run_directory(run_id)?
            .read_json(Path::new("run.json"))?;
        validate_context(&run.context)?;
        if &run.context.run_id != run_id {
            return Err(ScienceError::Invalid(
                "run record identity does not match requested run".into(),
            ));
        }
        Ok(run)
    }

    pub fn load_run_bounded(&self, run_id: &RunId, max_json_bytes: u64) -> Result<RunRecord> {
        let run: RunRecord = self.open_run_directory(run_id)?.read_json_bounded(
            Path::new("run.json"),
            max_json_bytes.min(Self::MAX_DOSSIER_RUN_RECORD_BYTES),
        )?;
        validate_context(&run.context)?;
        if &run.context.run_id != run_id {
            return Err(ScienceError::Invalid(
                "run record identity does not match requested run".into(),
            ));
        }
        Ok(run)
    }

    pub fn transition(
        &self,
        run_id: &RunId,
        state: RunState,
        reason: Option<String>,
    ) -> Result<RunRecord> {
        let mut run = self.load_run(run_id)?;
        if run.state.terminal() {
            return Err(ScienceError::Invalid(
                "terminal run cannot transition".into(),
            ));
        }
        run.state = state;
        run.terminal_reason = reason;
        self.open_run_directory(run_id)?
            .replace_json_atomic(Path::new("run.json"), &run)?;
        Ok(run)
    }

    /// Make Succeeded the final visible commit and reconcile the narrow case
    /// where atomic replacement became visible but the directory-sync call
    /// reported an error. Returning an error while a read-back is already
    /// Succeeded would create an API/durable split that callers cannot safely
    /// roll back because terminal states are immutable.
    pub fn transition_succeeded_verified(&self, run_id: &RunId) -> Result<RunRecord> {
        match self.transition(run_id, RunState::Succeeded, None) {
            Ok(run) => Ok(run),
            Err(error) => match self.load_run(run_id) {
                Ok(run) if run.state == RunState::Succeeded => Ok(run),
                _ => Err(error),
            },
        }
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
        let mut events: Vec<Event> = match run_dir.read_json(Path::new("events.json")) {
            Ok(events) => events,
            Err(error) => {
                if fail_run {
                    let _ = self.transition(
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
                let _ = self.transition(
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
        let events: Vec<Event> = self
            .open_run_directory(run_id)?
            .read_json(Path::new("events.json"))?;
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
        self.assert_owner(&approval.project_id, &approval.run_id, &approval.owner_id)?;
        let run_dir = self.open_run_directory(&approval.run_id)?;
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
        self.assert_owner(project, run_id, owner)?;
        let run_dir = self.open_run_directory(run_id)?;
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
        item.decision = decision;
        item.decided_at = Some(Utc::now());
        let result = item.clone();
        run_dir.replace_json_atomic(Path::new("approvals.json"), &items)?;
        Ok(result)
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
        self.assert_owner(project, run_id, owner)?;
        validate_relative(relative)?;
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let run_dir = self.open_run_directory(run_id)?;
        let run: RunRecord = run_dir.read_json(Path::new("run.json"))?;
        if run.state.terminal() {
            return Err(ScienceError::Invalid(
                "terminal science run outputs are immutable".into(),
            ));
        }
        let mut items: Vec<Artifact> = run_dir.read_json(Path::new("artifacts.json"))?;
        validate_artifacts(&items, run_id)?;
        if items
            .iter()
            .any(|artifact| artifact.relative_path == relative)
        {
            return Err(ScienceError::Invalid(
                "artifact path is already registered to run".into(),
            ));
        }
        let artifact_dir = run_dir.open_directory(Path::new("artifacts"))?;
        artifact_dir.write_new_atomic(relative, bytes)?;
        let artifact = Artifact {
            run_id: run_id.clone(),
            call_id: call,
            relative_path: relative.to_path_buf(),
            sha256: hex_sha256(bytes),
            bytes: bytes.len() as u64,
            mime: mime.into(),
            preview: preview.into(),
        };
        items.push(artifact.clone());
        if let Err(error) = run_dir.replace_json_atomic(Path::new("artifacts.json"), &items) {
            let _ = artifact_dir.unlink_file(relative);
            return Err(error);
        }
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
        self.assert_owner(project, run_id, owner)?;
        for relative in relative_paths {
            validate_relative(relative)?;
        }
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let run_dir = self.open_run_directory(run_id)?;
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
    /// artifact registry, evidence, and provenance before unlinking the known
    /// payload names ensures a subsequent Failed terminal cannot retain a
    /// partially authoritative result. Corrupt metadata is overwritten rather
    /// than parsed during rollback so the corruption that triggered rollback
    /// cannot prevent de-publication.
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
        let artifact_dir = run_dir.open_directory(Path::new("artifacts"))?;
        for relative in relative_paths {
            artifact_dir.unlink_file(relative)?;
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
        if run.state.terminal() {
            return Err(ScienceError::Invalid(
                "terminal science run outputs are immutable".into(),
            ));
        }
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
        if run.state.terminal() {
            return Err(ScienceError::Invalid(
                "terminal science run outputs are immutable".into(),
            ));
        }
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
        let items: Vec<Artifact> = self
            .open_run_directory(run_id)?
            .read_json(Path::new("artifacts.json"))?;
        validate_artifacts(&items, run_id)?;
        Ok(items)
    }
    pub fn add_preview(&self, preview: preview::PreviewRecord) -> Result<()> {
        preview.run_id.validate()?;
        preview.call_id.validate()?;
        validate_relative(&preview.relative_path)?;
        let run_dir = self.open_run_directory(&preview.run_id)?;
        let mut items: Vec<preview::PreviewRecord> =
            run_dir.read_json(Path::new("previews.json"))?;
        validate_previews(&items, &preview.run_id)?;
        items.push(preview);
        run_dir.replace_json_atomic(Path::new("previews.json"), &items)
    }
    /// Preview records for a run. Runs created before preview support have
    /// no `previews.json`; they read as empty rather than erroring.
    pub fn previews(&self, run_id: &RunId) -> Result<Vec<preview::PreviewRecord>> {
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
        let items: Vec<Evidence> = self
            .open_run_directory(run_id)?
            .read_json(Path::new("evidence.json"))?;
        validate_run_ids(items.iter().map(|item| &item.run_id), run_id, "evidence")?;
        Ok(items)
    }
    pub fn provenance(&self, run_id: &RunId) -> Result<Vec<Provenance>> {
        let items: Vec<Provenance> = self
            .open_run_directory(run_id)?
            .read_json(Path::new("provenance.json"))?;
        validate_run_ids(items.iter().map(|item| &item.run_id), run_id, "provenance")?;
        Ok(items)
    }
    pub fn approvals(&self, run_id: &RunId) -> Result<Vec<Approval>> {
        let items: Vec<Approval> = self
            .open_run_directory(run_id)?
            .read_json(Path::new("approvals.json"))?;
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
        self.assert_owner(project, run_id, owner)?;
        let run = self.load_run(run_id)?;
        if run.state != required_state {
            return Err(ScienceError::Invalid(
                "artifact bytes are unavailable in the run's current state".into(),
            ));
        }
        validate_relative(relative)?;
        let run_dir = self.open_run_directory(run_id)?;
        let artifacts: Vec<Artifact> = match max_bytes {
            Some(_) => run_dir.read_json_bounded(
                Path::new("artifacts.json"),
                Self::MAX_DOSSIER_REGISTRY_BYTES,
            )?,
            None => run_dir.read_json(Path::new("artifacts.json"))?,
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
        let items: Vec<Artifact> = self.open_run_directory(run_id)?.read_json_bounded(
            Path::new("artifacts.json"),
            max_json_bytes.min(Self::MAX_DOSSIER_REGISTRY_BYTES),
        )?;
        validate_artifacts(&items, run_id)?;
        if items.len() > max_items {
            return Err(ScienceError::Invalid(
                "artifact registry exceeds the dossier item limit".into(),
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
        let items: Vec<Evidence> = self.open_run_directory(run_id)?.read_json_bounded(
            Path::new("evidence.json"),
            max_json_bytes.min(Self::MAX_DOSSIER_REGISTRY_BYTES),
        )?;
        validate_run_ids(items.iter().map(|item| &item.run_id), run_id, "evidence")?;
        if items.len() > max_items {
            return Err(ScienceError::Invalid(
                "evidence registry exceeds the dossier item limit".into(),
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
        let items: Vec<Provenance> = self.open_run_directory(run_id)?.read_json_bounded(
            Path::new("provenance.json"),
            max_json_bytes.min(Self::MAX_DOSSIER_REGISTRY_BYTES),
        )?;
        validate_run_ids(items.iter().map(|item| &item.run_id), run_id, "provenance")?;
        if items.len() > max_items {
            return Err(ScienceError::Invalid(
                "provenance registry exceeds the dossier item limit".into(),
            ));
        }
        Ok(items)
    }

    pub fn recover_interrupted(&self, run_id: &RunId) -> Result<RunRecord> {
        let run = self.load_run(run_id)?;
        if run.state.terminal() {
            return Ok(run);
        }
        let run_dir = self.open_run_directory(run_id)?;
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
        self.transition(
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
        use std::os::unix::fs::OpenOptionsExt as _;

        // Resolve platform aliases such as macOS /var -> /private/var once,
        // then re-open every resulting component without following links.
        let canonical = dunce::canonicalize(path)?;
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

/// Windows retains directory/file handles opened with
/// `FILE_FLAG_OPEN_REPARSE_POINT`, rejects every reparse-point component, and
/// checks both final handle paths and stable file identities around pathname
/// publication. Unix's openat/linkat backend remains the stronger reference
/// implementation; Windows coverage is source-level until exercised by CI on
/// a Windows host.
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

    fn try_clone(&self) -> Result<Self> {
        Err(ScienceError::FeatureDisabled(
            "confined artifact I/O has no backend for this platform".into(),
        ))
    }
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

    #[cfg(unix)]
    #[test]
    fn registered_symlink_artifact_is_rejected_on_read() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let run = store
            .create_run(context(temp.path(), "a", "alice"))
            .unwrap();
        store
            .put_artifact(
                &run.context.project_id,
                &run.context.run_id,
                "alice",
                CallId::new("c"),
                Path::new("artifact.txt"),
                b"safe",
                "text/plain",
                "text",
            )
            .unwrap();
        store
            .transition(&run.context.run_id, RunState::Succeeded, None)
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
                    CallId::new("write"),
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
                CallId::new("read"),
                Path::new("registered.txt"),
                b"registered",
                "text/plain",
                "text",
            )
            .unwrap();
        store
            .transition(&run.context.run_id, RunState::Succeeded, None)
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
                    CallId::new("call"),
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
        let call = CallId::new("call");
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

    #[cfg(unix)]
    #[test]
    fn artifact_read_hashes_the_same_open_handle_and_requires_success() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("store"));
        let run = store
            .create_run(context(temp.path(), "a", "alice"))
            .unwrap();
        store
            .put_artifact(
                &run.context.project_id,
                &run.context.run_id,
                "alice",
                CallId::new("call"),
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
            .transition(&run.context.run_id, RunState::Succeeded, None)
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
    fn terminal_run_scientific_outputs_are_immutable() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let run = store
            .create_run(context(temp.path(), "a", "alice"))
            .unwrap();
        store
            .transition(&run.context.run_id, RunState::Succeeded, None)
            .unwrap();

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
        assert!(
            science
                .shares_root_capability_with(&original_project)
                .unwrap()
        );

        let retained = workspace.join("retained-store");
        fs::rename(&root, &retained).unwrap();
        fs::create_dir(&root).unwrap();
        let replacement_project = project::ProjectStore::new_confined(&root, &workspace).unwrap();

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
    }
}
