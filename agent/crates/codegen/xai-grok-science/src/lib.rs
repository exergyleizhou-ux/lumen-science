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
    io::Write,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};
use uuid::Uuid;

pub mod api;
pub mod connector;
pub mod connectors;
pub mod csv;
pub mod import;
pub mod preview;
pub mod review;
pub mod transport;

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
}

pub type Result<T> = std::result::Result<T, ScienceError>;

macro_rules! id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
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
    writes: Arc<Mutex<()>>,
}

impl ScienceStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            writes: Arc::new(Mutex::new(())),
        }
    }

    pub fn create_run(&self, context: RunContext) -> Result<RunRecord> {
        validate_context(&context)?;
        let record = RunRecord {
            schema_version: SCHEMA_VERSION,
            context,
            state: RunState::Created,
            terminal_reason: None,
        };
        let dir = self.run_dir(&record.context.run_id);
        fs::create_dir_all(dir.join("artifacts"))?;
        write_json_atomic(&dir.join("run.json"), &record)?;
        write_json_atomic::<Vec<Event>>(&dir.join("events.json"), &Vec::new())?;
        write_json_atomic::<Vec<Artifact>>(&dir.join("artifacts.json"), &Vec::new())?;
        write_json_atomic::<Vec<Evidence>>(&dir.join("evidence.json"), &Vec::new())?;
        write_json_atomic::<Vec<Provenance>>(&dir.join("provenance.json"), &Vec::new())?;
        write_json_atomic::<Vec<Approval>>(&dir.join("approvals.json"), &Vec::new())?;
        write_json_atomic::<Vec<preview::PreviewRecord>>(&dir.join("previews.json"), &Vec::new())?;
        Ok(record)
    }

    pub fn load_run(&self, run_id: &RunId) -> Result<RunRecord> {
        read_json(&self.run_dir(run_id).join("run.json"))
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
        write_json_atomic(&self.run_dir(run_id).join("run.json"), &run)?;
        Ok(run)
    }

    pub fn append_event(
        &self,
        run_id: &RunId,
        actor: impl Into<String>,
        kind: impl Into<String>,
        payload: serde_json::Value,
    ) -> Result<Event> {
        let _guard = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("science store write lock poisoned".into()))?;
        let path = self.run_dir(run_id).join("events.json");
        let mut events: Vec<Event> = match read_json(&path) {
            Ok(events) => events,
            Err(error) => {
                let _ = self.transition(
                    run_id,
                    RunState::Failed,
                    Some(format!("event persistence failed: {error}")),
                );
                return Err(error);
            }
        };
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
        if let Err(error) = write_json_atomic(&path, &events) {
            let _ = self.transition(
                run_id,
                RunState::Failed,
                Some(format!("event persistence failed: {error}")),
            );
            return Err(error);
        }
        Ok(event)
    }

    pub fn events_after(&self, run_id: &RunId, after: u64, limit: usize) -> Result<Vec<Event>> {
        if limit == 0 || limit > 1_000 {
            return Err(ScienceError::Invalid("event limit must be 1..=1000".into()));
        }
        let events: Vec<Event> = read_json(&self.run_dir(run_id).join("events.json"))?;
        Ok(events
            .into_iter()
            .filter(|event| event.seq > after)
            .take(limit)
            .collect())
    }

    pub fn request_approval(&self, approval: Approval) -> Result<()> {
        if approval.decision != ApprovalDecision::Pending {
            return Err(ScienceError::Invalid("new approval must be pending".into()));
        }
        self.assert_owner(&approval.project_id, &approval.run_id, &approval.owner_id)?;
        let path = self.run_dir(&approval.run_id).join("approvals.json");
        let mut items: Vec<Approval> = read_json(&path)?;
        if items.iter().any(|item| item.call_id == approval.call_id) {
            return Err(ScienceError::Invalid("duplicate approval call".into()));
        }
        items.push(approval);
        write_json_atomic(&path, &items)
    }

    pub fn decide_approval(
        &self,
        project: &ProjectId,
        run_id: &RunId,
        owner: &str,
        call: &CallId,
        decision: ApprovalDecision,
    ) -> Result<Approval> {
        if !decision.terminal() {
            return Err(ScienceError::Invalid("decision must be terminal".into()));
        }
        self.assert_owner(project, run_id, owner)?;
        let path = self.run_dir(run_id).join("approvals.json");
        let mut items: Vec<Approval> = read_json(&path)?;
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
        write_json_atomic(&path, &items)?;
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
        self.assert_owner(project, run_id, owner)?;
        validate_relative(relative)?;
        let target = self.run_dir(run_id).join("artifacts").join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        write_bytes_atomic(&target, bytes)?;
        let artifact = Artifact {
            run_id: run_id.clone(),
            call_id: call,
            relative_path: relative.to_path_buf(),
            sha256: hex_sha256(bytes),
            bytes: bytes.len() as u64,
            mime: mime.into(),
            preview: preview.into(),
        };
        let path = self.run_dir(run_id).join("artifacts.json");
        let mut items: Vec<Artifact> = read_json(&path)?;
        items.push(artifact.clone());
        write_json_atomic(&path, &items)?;
        Ok(artifact)
    }

    pub fn add_evidence(&self, evidence: Evidence) -> Result<()> {
        append_json(
            &self.run_dir(&evidence.run_id).join("evidence.json"),
            evidence,
        )
    }
    pub fn add_provenance(&self, provenance: Provenance) -> Result<()> {
        append_json(
            &self.run_dir(&provenance.run_id).join("provenance.json"),
            provenance,
        )
    }
    pub fn artifacts(&self, run_id: &RunId) -> Result<Vec<Artifact>> {
        read_json(&self.run_dir(run_id).join("artifacts.json"))
    }
    pub fn add_preview(&self, preview: preview::PreviewRecord) -> Result<()> {
        append_json(
            &self.run_dir(&preview.run_id).join("previews.json"),
            preview,
        )
    }
    /// Preview records for a run. Runs created before preview support have
    /// no `previews.json`; they read as empty rather than erroring.
    pub fn previews(&self, run_id: &RunId) -> Result<Vec<preview::PreviewRecord>> {
        match read_json(&self.run_dir(run_id).join("previews.json")) {
            Ok(items) => Ok(items),
            Err(ScienceError::Io(ref error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(Vec::new())
            }
            Err(error) => Err(error),
        }
    }
    pub fn evidence(&self, run_id: &RunId) -> Result<Vec<Evidence>> {
        read_json(&self.run_dir(run_id).join("evidence.json"))
    }
    pub fn provenance(&self, run_id: &RunId) -> Result<Vec<Provenance>> {
        read_json(&self.run_dir(run_id).join("provenance.json"))
    }
    pub fn approvals(&self, run_id: &RunId) -> Result<Vec<Approval>> {
        read_json(&self.run_dir(run_id).join("approvals.json"))
    }
    pub fn artifact_bytes(
        &self,
        project: &ProjectId,
        run_id: &RunId,
        owner: &str,
        relative: &Path,
    ) -> Result<Vec<u8>> {
        self.assert_owner(project, run_id, owner)?;
        validate_relative(relative)?;
        let artifacts = self.artifacts(run_id)?;
        if !artifacts.iter().any(|item| item.relative_path == relative) {
            return Err(ScienceError::Invalid(
                "artifact is not registered to run".into(),
            ));
        }
        let artifact_root = self.run_dir(run_id).join("artifacts");
        let target = artifact_root.join(relative);
        let metadata = fs::symlink_metadata(&target)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(ScienceError::Invalid(
                "artifact must be a regular file, not a symlink".into(),
            ));
        }
        let canonical_root = dunce::canonicalize(&artifact_root)?;
        let canonical_target = dunce::canonicalize(&target)?;
        if !canonical_target.starts_with(&canonical_root) {
            return Err(ScienceError::Invalid(
                "artifact resolved outside its run root".into(),
            ));
        }
        Ok(fs::read(canonical_target)?)
    }

    pub fn recover_interrupted(&self, run_id: &RunId) -> Result<RunRecord> {
        let run = self.load_run(run_id)?;
        if run.state.terminal() {
            return Ok(run);
        }
        let approvals_path = self.run_dir(run_id).join("approvals.json");
        let mut approvals: Vec<Approval> = read_json(&approvals_path)?;
        let mut approvals_changed = false;
        for approval in &mut approvals {
            if approval.decision == ApprovalDecision::Pending {
                approval.decision = ApprovalDecision::Interrupted;
                approval.decided_at = Some(Utc::now());
                approvals_changed = true;
            }
        }
        if approvals_changed {
            write_json_atomic(&approvals_path, &approvals)?;
        }
        self.transition(
            run_id,
            RunState::Interrupted,
            Some("process restarted before terminal state".into()),
        )
    }

    fn assert_owner(&self, project: &ProjectId, run_id: &RunId, owner: &str) -> Result<()> {
        let run = self.load_run(run_id)?;
        if &run.context.project_id != project || run.context.owner_id != owner {
            return Err(ScienceError::Ownership);
        }
        Ok(())
    }
    fn run_dir(&self, run_id: &RunId) -> PathBuf {
        self.root.join("runs").join(&run_id.0)
    }
}

fn validate_context(context: &RunContext) -> Result<()> {
    if context.run_id.0.is_empty()
        || context.project_id.0.is_empty()
        || context.session_id.is_empty()
        || context.owner_id.is_empty()
    {
        return Err(ScienceError::Invalid(
            "ids and owner must be non-empty".into(),
        ));
    }
    Ok(())
}
pub(crate) fn validate_relative(path: &Path) -> Result<()> {
    if path.is_absolute()
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
fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    write_bytes_atomic(path, &serde_json::to_vec_pretty(value)?)
}
fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| ScienceError::Invalid("path has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".science-{}.tmp", Uuid::new_v4()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&temp, path)?;
    Ok(())
}
fn append_json<T: Serialize + DeserializeOwned>(path: &Path, value: T) -> Result<()> {
    let mut items: Vec<T> = read_json(path)?;
    items.push(value);
    write_json_atomic(path, &items)
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
        let outside = temp.path().join("outside-secret");
        fs::write(&outside, b"secret").unwrap();
        let target = store
            .run_dir(&run.context.run_id)
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
    fn corrupt_event_store_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let run = store
            .create_run(context(temp.path(), "a", "alice"))
            .unwrap();
        fs::write(
            store.run_dir(&run.context.run_id).join("events.json"),
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
}
