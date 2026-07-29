//! Durable store for ResearchProject + EvidenceGraph + Claims (WP-2 product path).
//!
//! Layout under `root/projects/{project_id}/`:
//!   project.json
//!   graph.json
//!   artifacts.json
//!   claims/{claim_id}.json
//!   reviews/{operation_id}.json
//!
//! Records only — SessionActor remains sole execution authority.

use super::capability::{PinnedDirectory, ProjectWriteFileLock};
use super::claim::{Claim, ClaimStatus};
use super::evidence_graph::{
    EdgeKind, EvidenceEdge, EvidenceGraph, EvidenceNode, NodeId, NodeKind, validate_sha256_hex,
};
use super::model::{OwnerId, ProjectId, ProjectStatus, ResearchProject, validate_project_id};
use crate::features::{FeatureGates, ScienceFeature};
use crate::{Result, ScienceError};
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, OnceLock},
};
#[cfg(test)]
use std::{fs, io::Write};
use uuid::Uuid;

/// Prefix for the unique temp files used by durable record writes.
const TEMP_PREFIX: &str = ".project-";

fn validate_record_stem(value: &str, field: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(ScienceError::Invalid(format!(
            "{field} must be 1..=128 [A-Za-z0-9_-] characters"
        )));
    }
    Ok(())
}

/// Process-wide write locks, keyed by store root.
///
/// The ACP handlers build a fresh `ProjectStore` for every request, so a
/// per-instance mutex would serialise nothing. Keying by root makes every
/// writer in this process queue behind the same guard, which is what keeps a
/// read-modify-write of `graph.json` from losing a concurrent update.
pub(crate) fn write_lock_for(root: &Path) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
    // Canonicalise so two spellings of one root share a lock; fall back to an
    // absolute path while the root does not exist yet.
    let key = dunce::canonicalize(root)
        .or_else(|_| std::path::absolute(root))
        .unwrap_or_else(|_| root.to_path_buf());
    let mut locks = LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Arc::clone(locks.entry(key).or_insert_with(|| Arc::new(Mutex::new(()))))
}

/// What `recover_project` found and repaired after an interrupted mutation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRecoveryReport {
    /// Claims whose evidence node was missing from the graph and was rebuilt
    /// from the claim record.
    pub claim_nodes_restored: Vec<String>,
    /// Claim nodes in the graph with no claim record behind them.
    pub orphan_nodes_removed: Vec<String>,
    /// Edges dropped because an endpoint no longer exists.
    pub orphan_edges_removed: usize,
    /// Claims advanced to EvidenceAttached because evidence was already linked.
    pub claims_advanced: Vec<String>,
    /// Temp files left behind by a write that never completed.
    pub stale_temp_files_removed: usize,
}

impl ProjectRecoveryReport {
    pub fn repaired(&self) -> bool {
        !self.claim_nodes_restored.is_empty()
            || !self.orphan_nodes_removed.is_empty()
            || self.orphan_edges_removed > 0
            || !self.claims_advanced.is_empty()
            || self.stale_temp_files_removed > 0
    }
}

/// An artifact digest that has been registered against a project.
///
/// `attach_evidence` refuses to cite a digest that is not registered here:
/// an evidence graph must never point at an artifact nobody produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredArtifact {
    pub project_id: ProjectId,
    /// Canonical digest: exactly 64 lowercase hex characters.
    pub sha256: String,
    pub label: String,
    pub run_id: Option<String>,
    pub registered_by: String,
    pub registered_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ProjectStore {
    root: PathBuf,
    gates: FeatureGates,
    /// Opened once and retained for the lifetime of every clone. Product
    /// record I/O is relative to this capability, never to `root`.
    confined: std::result::Result<Arc<PinnedDirectory>, Arc<str>>,
    /// Serialises writers to this root; see [`write_lock_for`].
    writes: Arc<Mutex<()>>,
}

/// Proof that the process-wide write lock for one retained project root is
/// held for the lifetime of this value.
///
/// Fields are private on purpose: callers can receive this proof only from
/// [`ProjectStore::with_owned_project_revision_guarded`]. Workflow execution
/// uses it to avoid trying to re-lock the same non-reentrant mutex while still
/// keeping project mutation, execution, and authority commit in one critical
/// section.
pub struct HeldProjectRootWriteGuard<'a> {
    writes: &'a Arc<Mutex<()>>,
    _guard: ProjectStoreWriteGuard<'a>,
}

impl HeldProjectRootWriteGuard<'_> {
    pub(crate) fn authorizes(&self, writes: &Arc<Mutex<()>>) -> bool {
        Arc::ptr_eq(self.writes, writes)
    }
}

/// One complete project-store writer lease.
///
/// The cross-process file lock is declared first so it is released while the
/// process mutex is still held. No local writer can enter the small hand-off
/// window between releasing `flock` and releasing the mutex.
pub(super) struct ProjectStoreWriteGuard<'a> {
    _cross_process: ProjectWriteFileLock,
    _process: MutexGuard<'a, ()>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectBundle {
    pub project: ResearchProject,
    pub graph: EvidenceGraph,
    pub claims: Vec<Claim>,
}

impl ProjectStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let writes = write_lock_for(&root);
        let confined = PinnedDirectory::open_or_create(&root)
            .map(Arc::new)
            .map_err(|error| Arc::<str>::from(error.to_string()));
        Self {
            root,
            gates: FeatureGates::default(),
            confined,
            writes,
        }
    }

    /// Construct a product store whose retained root capability is proven to
    /// be the same directory as a canonical path below `workspace`.
    ///
    /// Unlike [`ProjectStore::new`], capability acquisition is eager and
    /// fallible. ACP callers should use this constructor after resolving the
    /// workspace boundary: a path check followed by `new` would otherwise
    /// leave an ancestor-swap window between those two operations.
    pub fn new_confined(root: impl Into<PathBuf>, workspace: &Path) -> Result<Self> {
        let root = root.into();
        let writes = write_lock_for(&root);
        let confined = Arc::new(PinnedDirectory::open_or_create_within(&root, workspace)?);
        Ok(Self {
            root,
            gates: FeatureGates::default(),
            confined: Ok(confined),
            writes,
        })
    }

    pub fn with_gates(mut self, gates: FeatureGates) -> Self {
        self.gates = gates;
        self
    }

    pub fn gates(&self) -> &FeatureGates {
        &self.gates
    }

    pub fn gates_mut(&mut self) -> &mut FeatureGates {
        &mut self.gates
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn root_identity(&self) -> Result<crate::StoreRootIdentity> {
        self.confined()?.identity()
    }

    #[cfg(test)]
    pub(super) fn project_dir(&self, id: &ProjectId) -> PathBuf {
        self.root.join("projects").join(&id.0)
    }

    fn confined(&self) -> Result<&PinnedDirectory> {
        self.confined.as_deref().map_err(|error| {
            ScienceError::Invalid(format!("project store capability is unavailable: {error}"))
        })
    }

    fn project_relative(id: &ProjectId) -> Result<PathBuf> {
        validate_project_id(&id.0)?;
        Ok(PathBuf::from("projects").join(&id.0))
    }

    fn project_record(id: &ProjectId, name: &str) -> Result<PathBuf> {
        Ok(Self::project_relative(id)?.join(name))
    }

    fn claim_record(id: &ProjectId, claim_id: &str) -> Result<PathBuf> {
        validate_record_stem(claim_id, "claim id")?;
        Ok(Self::project_relative(id)?
            .join("claims")
            .join(format!("{claim_id}.json")))
    }

    fn read_confined<T: DeserializeOwned>(&self, relative: &Path) -> Result<Option<T>> {
        self.confined()?
            .read_optional(relative)?
            .map(|bytes| Ok(serde_json::from_slice(&bytes)?))
            .transpose()
    }

    fn write_confined<T: Serialize>(&self, relative: &Path, value: &T) -> Result<()> {
        self.confined()?
            .replace_atomic(relative, &serde_json::to_vec_pretty(value)?)
    }

    fn write_new_confined<T: Serialize>(&self, relative: &Path, value: &T) -> Result<()> {
        self.confined()?
            .write_new_atomic(relative, &serde_json::to_vec_pretty(value)?)
    }

    pub(super) fn list_confined(&self, relative: &Path) -> Result<Vec<std::ffi::OsString>> {
        self.confined()?.list_names(relative)
    }

    pub(super) fn read_confined_record<T: DeserializeOwned>(
        &self,
        relative: &Path,
    ) -> Result<Option<T>> {
        self.read_confined(relative)
    }

    pub(super) fn write_new_confined_record<T: Serialize>(
        &self,
        relative: &Path,
        value: &T,
    ) -> Result<()> {
        self.write_new_confined(relative, value)
    }

    /// Take the per-root write lock for the duration of one mutation.
    ///
    /// Unix writers retain both the process-wide mutex and a descriptor-bound
    /// blocking `flock`, so independently launched Lumen processes cannot
    /// interleave a revision recheck with another project mutation. Other
    /// platforms currently retain the process mutex only and make no
    /// cross-process safety claim.
    ///
    /// Fails closed on poisoning or an unsafe/unavailable Unix lock record: a
    /// writer that panicked mid-mutation may have left records half-applied,
    /// and callers must recover before writing more on top of them.
    pub(super) fn write_guard(&self) -> Result<ProjectStoreWriteGuard<'_>> {
        let process = self
            .writes
            .lock()
            .map_err(|_| ScienceError::Invalid("project store write lock poisoned".into()))?;
        let cross_process = self.confined()?.lock_project_writes()?;
        Ok(ProjectStoreWriteGuard {
            _cross_process: cross_process,
            _process: process,
        })
    }

    /// Durably replace `path` with `value`.
    ///
    /// - unique temp name, so two writers of the same record never collide on
    ///   one temp path and clobber each other's partial bytes;
    /// - `sync_all` before the rename, so the bytes are on disk before
    ///   anything points at them;
    /// - `sync_dir` after it, so the rename itself survives a power loss;
    /// - temp file removed on any failure, so a failed write leaves no litter.
    ///
    /// Callers must already hold the write guard.
    #[cfg(test)]
    pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| ScienceError::Invalid("record path has no parent".into()))?;
        fs::create_dir_all(parent)?;
        let bytes = serde_json::to_vec_pretty(value)?;
        let temp = parent.join(format!("{TEMP_PREFIX}{}.tmp", Uuid::new_v4()));
        let staged = (|| -> Result<()> {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            Ok(())
        })();
        if let Err(error) = staged {
            let _ = fs::remove_file(&temp);
            return Err(error);
        }
        if let Err(error) = fs::rename(&temp, path) {
            let _ = fs::remove_file(&temp);
            return Err(error.into());
        }
        crate::sync_dir(parent);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
        let bytes = fs::read(path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// The temp-file prefix used by [`ProjectStore::write_json`], so other
    /// stores rooted here sweep the same litter after an interrupted write.
    #[cfg(test)]
    pub(crate) const fn temp_prefix() -> &'static str {
        TEMP_PREFIX
    }

    // ── Unlocked record writers ───────────────────────────────────
    //
    // These assume the caller holds the write guard. Public mutators take the
    // guard once and call these, because `std::sync::Mutex` is not reentrant:
    // a multi-file mutation that re-entered a locking method would deadlock.

    pub(super) fn write_project_file(&self, project: &ResearchProject) -> Result<()> {
        self.write_confined(
            &Self::project_record(&project.project_id, "project.json")?,
            project,
        )
    }

    pub(super) fn write_graph_file(&self, graph: &EvidenceGraph) -> Result<()> {
        graph
            .validate_integrity()
            .map_err(|error| ScienceError::Invalid(format!("evidence graph invalid: {error}")))?;
        self.write_confined(
            &Self::project_record(&graph.project_id, "graph.json")?,
            graph,
        )
    }

    pub(super) fn write_claim_file(&self, claim: &Claim) -> Result<()> {
        self.write_confined(
            &Self::claim_record(&claim.project_id, &claim.claim_id)?,
            claim,
        )
    }

    /// Create a draft research project with empty evidence graph.
    pub fn create_project(
        &self,
        owner_id: impl Into<String>,
        title: impl Into<String>,
        research_question: impl Into<String>,
    ) -> Result<ResearchProject> {
        let _guard = self.write_guard()?;
        self.create_project_inner(owner_id, title, research_question)
    }

    /// Caller must hold the write guard.
    pub(super) fn create_project_inner(
        &self,
        owner_id: impl Into<String>,
        title: impl Into<String>,
        research_question: impl Into<String>,
    ) -> Result<ResearchProject> {
        self.gates.require(ScienceFeature::ResearchProject)?;
        let project_id = ProjectId(Uuid::now_v7().to_string());
        let mut project = ResearchProject::new(
            project_id.clone(),
            OwnerId(owner_id.into()),
            title.into(),
            research_question.into(),
        );
        let graph = EvidenceGraph::new(project_id.clone());
        project.evidence_graph_id = Some(format!("graph-{}", project_id.0));
        // Graph first: a project whose graph is missing cannot be recovered
        // from the project record, but an unreferenced empty graph is inert.
        self.write_graph_file(&graph)?;
        self.write_project_file(&project)?;
        Ok(project)
    }

    pub fn load_project(&self, project_id: &ProjectId) -> Result<ResearchProject> {
        self.gates.require(ScienceFeature::ResearchProject)?;
        let path = Self::project_record(project_id, "project.json")?;
        let project: ResearchProject = self
            .read_confined(&path)?
            .ok_or_else(|| ScienceError::Invalid(format!("project not found: {}", project_id.0)))?;
        if project.project_id != *project_id {
            return Err(ScienceError::Ownership);
        }
        Ok(project)
    }

    pub(super) fn project_exists(&self, project_id: &ProjectId) -> Result<bool> {
        Ok(self
            .confined()?
            .read_optional(&Self::project_record(project_id, "project.json")?)?
            .is_some())
    }

    /// Assert that `owner_id` owns `project_id` without exposing whether the
    /// project exists.
    ///
    /// This is the single ownership gate for read-only project and evidence
    /// queries. A missing record, an unreadable/corrupt record, a record whose
    /// embedded project id does not match its directory, and an owner mismatch
    /// all return the same [`ScienceError::Ownership`]. Callers therefore
    /// cannot use the error shape as a project-existence oracle.
    pub fn assert_project_owner(&self, project_id: &ProjectId, owner_id: &str) -> Result<()> {
        self.load_owned_project(project_id, owner_id).map(drop)
    }

    /// Load and validate the aggregate root behind
    /// [`ProjectStore::assert_project_owner`].
    fn load_owned_project(
        &self,
        project_id: &ProjectId,
        owner_id: &str,
    ) -> Result<ResearchProject> {
        self.gates.require(ScienceFeature::ResearchProject)?;
        validate_project_id(&project_id.0)?;
        if owner_id.is_empty() {
            return Err(ScienceError::Ownership);
        }

        let path = Self::project_record(project_id, "project.json")?;
        let project: ResearchProject = match self.read_confined(&path) {
            Ok(Some(project)) => project,
            Ok(None) | Err(_) => return Err(ScienceError::Ownership),
        };
        if project.project_id != *project_id || project.owner_id.0 != owner_id {
            return Err(ScienceError::Ownership);
        }
        Ok(project)
    }

    /// Execute a read/commit closure while holding the process-wide write
    /// guard for this project store. This couples the owned aggregate snapshot
    /// to its content-addressed revision and prevents a concurrent mutation
    /// from changing the research question between an approval recheck and an
    /// actor-owned derived-artifact commit.
    pub fn with_owned_project_revision<T>(
        &self,
        project_id: &ProjectId,
        owner_id: &str,
        operation: impl FnOnce(&ResearchProject, &str) -> Result<T>,
    ) -> Result<T> {
        self.with_owned_project_revision_guarded(
            project_id,
            owner_id,
            |project, revision, _guard| operation(project, revision),
        )
    }

    /// Execute an operation while retaining typed proof that this exact
    /// project's root write lock remains held.
    ///
    /// This is the cross-store transaction seam for actor-owned work that
    /// writes workflow records below the same retained root. The token cannot
    /// be constructed by protocol adapters and is rejected by an executor
    /// opened on any other root.
    pub fn with_owned_project_revision_guarded<T>(
        &self,
        project_id: &ProjectId,
        owner_id: &str,
        operation: impl FnOnce(&ResearchProject, &str, &HeldProjectRootWriteGuard<'_>) -> Result<T>,
    ) -> Result<T> {
        let guard = self.write_guard()?;
        let held_guard = HeldProjectRootWriteGuard {
            writes: &self.writes,
            _guard: guard,
        };
        let project = self.load_owned_project(project_id, owner_id)?;
        let revision = self.project_revision(project_id)?;
        operation(&project, &revision, &held_guard)
    }

    pub fn save_project(&self, project: &ResearchProject) -> Result<()> {
        self.gates.require(ScienceFeature::ResearchProject)?;
        let _guard = self.write_guard()?;
        self.save_project_inner(project)
    }

    /// Caller must hold the write guard.
    pub(super) fn save_project_inner(&self, project: &ResearchProject) -> Result<()> {
        self.gates.require(ScienceFeature::ResearchProject)?;
        self.write_project_file(project)
    }

    pub fn load_graph(&self, project_id: &ProjectId) -> Result<EvidenceGraph> {
        self.gates.require(ScienceFeature::EvidenceGraph)?;
        let path = Self::project_record(project_id, "graph.json")?;
        match self.read_confined(&path)? {
            Some(graph) => Ok(graph),
            None => Ok(EvidenceGraph::new(project_id.clone())),
        }
    }

    /// Persist a graph. Fails closed if the graph violates its structural
    /// invariants (dangling endpoint, self-edge, derivation cycle, or a
    /// non-canonical artifact digest), so a corrupt or legacy graph can never
    /// be written back under a new mutation.
    pub fn save_graph(&self, graph: &EvidenceGraph) -> Result<()> {
        self.gates.require(ScienceFeature::EvidenceGraph)?;
        let _guard = self.write_guard()?;
        self.write_graph_file(graph)
    }

    // ── Revision + operation ledger ───────────────────────────────

    /// A content-addressed revision for a project: the digest of every record
    /// that belongs to it.
    ///
    /// Used as a compare-and-swap token. Content addressing rather than a
    /// counter means it needs no schema change, cannot be forged by editing a
    /// field, and moves whenever *any* record moves — including ones written
    /// by a path that predates this API.
    pub fn project_revision(&self, project_id: &ProjectId) -> Result<String> {
        use sha2::{Digest, Sha256};
        validate_project_id(&project_id.0)?;
        let project_record = Self::project_record(project_id, "project.json")?;
        if self.confined()?.read_optional(&project_record)?.is_none() {
            return Err(ScienceError::Invalid(format!(
                "project not found: {}",
                project_id.0
            )));
        }
        let mut hasher = Sha256::new();
        hasher.update(project_id.0.as_bytes());
        for name in [
            "project.json",
            "graph.json",
            "artifacts.json",
            "migration.json",
        ] {
            hasher.update(name.as_bytes());
            match self
                .confined()?
                .read_optional(&Self::project_record(project_id, name)?)?
            {
                Some(bytes) => hasher.update(&bytes),
                None => hasher.update(b"<absent>"),
            }
        }
        // Child records, in a deterministic order. Reviews must move the
        // project revision just like claims; otherwise a review could be
        // appended behind a caller's compare-and-swap token without conflict.
        for child_name in ["claims", "reviews"] {
            let child_dir = Self::project_relative(project_id)?.join(child_name);
            for name in self.list_confined(&child_dir)? {
                let path = child_dir.join(&name);
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                hasher.update(child_name.as_bytes());
                if let Some(name) = name.to_str() {
                    hasher.update(name.as_bytes());
                }
                let bytes = self.confined()?.read_optional(&path)?.ok_or_else(|| {
                    ScienceError::Invalid("project child record disappeared during revision".into())
                })?;
                hasher.update(bytes);
            }
        }
        Ok(format!("{:x}", hasher.finalize()))
    }

    pub(super) fn operation_relative(operation_id: &str) -> Result<PathBuf> {
        super::mutation::validate_operation_id(operation_id)?;
        Ok(PathBuf::from("operations").join(format!("{operation_id}.json")))
    }

    /// The durable record of an already-applied operation id, if any.
    pub fn lookup_operation(
        &self,
        operation_id: &str,
    ) -> Result<Option<super::mutation::OperationRecord>> {
        super::mutation::validate_operation_id(operation_id)?;
        let path = Self::operation_relative(operation_id)?;
        let record: Option<super::mutation::OperationRecord> = self.read_confined(&path)?;
        if record
            .as_ref()
            .is_some_and(|record| record.operation_id != operation_id)
        {
            return Err(ScienceError::Ownership);
        }
        Ok(record)
    }

    /// Persist the idempotency record for an applied operation.
    /// Caller must hold the write guard.
    pub(super) fn record_operation(&self, record: &super::mutation::OperationRecord) -> Result<()> {
        super::mutation::validate_operation_id(&record.operation_id)?;
        let path = Self::operation_relative(&record.operation_id)?;
        self.write_new_confined(&path, record).map_err(|error| {
            if matches!(
                &error,
                ScienceError::Io(io) if io.kind() == std::io::ErrorKind::AlreadyExists
            ) {
                ScienceError::Invalid(format!(
                    "operation {} already recorded",
                    record.operation_id
                ))
            } else {
                error
            }
        })
    }

    // ── Artifact registry ─────────────────────────────────────────

    fn artifacts_relative(&self, project_id: &ProjectId) -> Result<PathBuf> {
        Self::project_record(project_id, "artifacts.json")
    }

    /// All artifact digests registered against a project, keyed by digest.
    pub fn list_artifacts(
        &self,
        project_id: &ProjectId,
    ) -> Result<BTreeMap<String, RegisteredArtifact>> {
        let path = self.artifacts_relative(project_id)?;
        let registry: BTreeMap<String, RegisteredArtifact> =
            self.read_confined(&path)?.unwrap_or_default();
        if registry
            .iter()
            .any(|(sha, record)| record.project_id != *project_id || record.sha256 != *sha)
        {
            return Err(ScienceError::Ownership);
        }
        Ok(registry)
    }

    /// Register an artifact digest against a project so evidence may cite it.
    ///
    /// Idempotent: re-registering the same digest for the same project returns
    /// the existing record rather than duplicating it.
    pub fn register_artifact(
        &self,
        project_id: &ProjectId,
        owner_id: &str,
        artifact_sha256: impl Into<String>,
        label: impl Into<String>,
        run_id: Option<String>,
    ) -> Result<RegisteredArtifact> {
        // Held across the read-modify-write so a concurrent registration of a
        // different digest is not lost.
        let _guard = self.write_guard()?;
        self.register_artifact_inner(project_id, owner_id, artifact_sha256, label, run_id)
    }

    /// Caller must hold the project-store write guard.
    pub(super) fn register_artifact_inner(
        &self,
        project_id: &ProjectId,
        owner_id: &str,
        artifact_sha256: impl Into<String>,
        label: impl Into<String>,
        run_id: Option<String>,
    ) -> Result<RegisteredArtifact> {
        self.gates.require(ScienceFeature::EvidenceGraph)?;
        let project = self.load_project(project_id)?;
        if project.owner_id.0 != owner_id {
            return Err(ScienceError::Ownership);
        }
        let sha = artifact_sha256.into();
        validate_sha256_hex(&sha).map_err(ScienceError::Invalid)?;
        let mut registry = self.list_artifacts(project_id)?;
        if let Some(existing) = registry.get(&sha) {
            if existing.project_id != *project_id {
                return Err(ScienceError::Invalid(format!(
                    "registered artifact {sha} is bound to project {}, not {}",
                    existing.project_id.0, project_id.0
                )));
            }
            return Ok(existing.clone());
        }
        let record = RegisteredArtifact {
            project_id: project_id.clone(),
            sha256: sha.clone(),
            label: label.into(),
            run_id,
            registered_by: owner_id.to_string(),
            registered_at: Utc::now(),
        };
        registry.insert(sha, record.clone());
        self.write_confined(&self.artifacts_relative(project_id)?, &registry)?;
        Ok(record)
    }

    /// Replace the complete registry while committing a newly-created
    /// migration aggregate. This is not exposed outside the project module;
    /// ordinary mutations must continue to use `register_artifact`.
    pub(super) fn write_artifact_registry_inner(
        &self,
        project_id: &ProjectId,
        registry: &BTreeMap<String, RegisteredArtifact>,
    ) -> Result<()> {
        if registry.iter().any(|(sha, record)| {
            validate_sha256_hex(sha).is_err()
                || record.project_id != *project_id
                || record.sha256 != *sha
        }) {
            return Err(ScienceError::Invalid(
                "migration artifact registry is malformed".into(),
            ));
        }
        self.write_confined(&self.artifacts_relative(project_id)?, registry)
    }

    pub(super) fn migration_relative(project_id: &ProjectId) -> Result<PathBuf> {
        Self::project_record(project_id, "migration.json")
    }

    pub(super) fn write_migration_manifest_inner(
        &self,
        project_id: &ProjectId,
        manifest: &super::migration::MigrationManifest,
    ) -> Result<()> {
        self.write_confined(&Self::migration_relative(project_id)?, manifest)
    }

    pub fn load_migration_manifest(
        &self,
        project_id: &ProjectId,
    ) -> Result<super::migration::MigrationManifest> {
        self.read_confined(&Self::migration_relative(project_id)?)?
            .ok_or_else(|| {
                ScienceError::Invalid(format!(
                    "migration manifest not found for project {}",
                    project_id.0
                ))
            })
    }

    fn migration_commit_relative(operation_id: &str) -> Result<PathBuf> {
        super::mutation::validate_operation_id(operation_id)?;
        Ok(PathBuf::from("migration-commits").join(format!("{operation_id}.json")))
    }

    /// Actor commit journal for crash recovery between project publication
    /// and the generic operation ledger. It is data, not an execution
    /// authority: only an already-Allowed authority run may create or recover
    /// one.
    pub fn lookup_migration_commit(
        &self,
        operation_id: &str,
    ) -> Result<Option<super::migration::MigrationCommit>> {
        let commit: Option<super::migration::MigrationCommit> =
            self.read_confined(&Self::migration_commit_relative(operation_id)?)?;
        if commit
            .as_ref()
            .is_some_and(|commit| commit.admission.operation_id() != operation_id)
        {
            return Err(ScienceError::Ownership);
        }
        Ok(commit)
    }

    /// Caller must hold the project-store write guard.
    pub(super) fn write_migration_commit_inner(
        &self,
        commit: &super::migration::MigrationCommit,
    ) -> Result<()> {
        let path = Self::migration_commit_relative(commit.admission.operation_id())?;
        match self.read_confined::<super::migration::MigrationCommit>(&path)? {
            Some(existing) if existing == *commit => Ok(()),
            Some(_) => Err(ScienceError::Invalid(format!(
                "migration commit {} conflicts with its durable journal",
                commit.admission.operation_id()
            ))),
            None => self.write_new_confined(&path, commit),
        }
    }

    /// The first other project that has registered this digest, if any.
    /// Used only to turn "unknown artifact" into a precise cross-project error.
    fn locate_artifact_elsewhere(
        &self,
        sha: &str,
        exclude: &ProjectId,
    ) -> Result<Option<ProjectId>> {
        for name in self.list_confined(Path::new("projects"))? {
            let Some(project_name) = name.to_str() else {
                continue;
            };
            if validate_project_id(project_name).is_err() {
                continue;
            }
            let path = PathBuf::from("projects")
                .join(project_name)
                .join("artifacts.json");
            let registry: BTreeMap<String, RegisteredArtifact> = match self.read_confined(&path) {
                Ok(Some(registry)) => registry,
                Ok(None) => continue,
                // A damaged sibling registry must not mask the real error.
                Err(_) => continue,
            };
            if let Some(record) = registry.get(sha)
                && record.project_id != *exclude
            {
                return Ok(Some(record.project_id.clone()));
            }
        }
        Ok(None)
    }

    /// Resolve a digest to its registration in this project, failing closed
    /// with a precise reason when it is unknown or owned by another project.
    fn require_registered_artifact(
        &self,
        project_id: &ProjectId,
        sha: &str,
    ) -> Result<RegisteredArtifact> {
        let registry = self.list_artifacts(project_id)?;
        match registry.get(sha) {
            Some(record) if record.project_id == *project_id => Ok(record.clone()),
            Some(record) => Err(ScienceError::Invalid(format!(
                "artifact {sha} is registered to project {}, not {}",
                record.project_id.0, project_id.0
            ))),
            None => match self.locate_artifact_elsewhere(sha, project_id)? {
                Some(other) => Err(ScienceError::Invalid(format!(
                    "artifact {sha} is registered to project {}, not {} — evidence may not cross projects",
                    other.0, project_id.0
                ))),
                None => Err(ScienceError::Invalid(format!(
                    "artifact {sha} is not registered in project {}; register it before citing it as evidence",
                    project_id.0
                ))),
            },
        }
    }

    pub fn transition_project(
        &self,
        project_id: &ProjectId,
        owner_id: &str,
        new_status: ProjectStatus,
    ) -> Result<ResearchProject> {
        // Guard first: load -> transition -> save is a read-modify-write, and
        // two concurrent transitions must not both validate against the same
        // stale status.
        let _guard = self.write_guard()?;
        self.transition_project_inner(project_id, owner_id, new_status)
    }

    /// Caller must hold the write guard.
    pub(super) fn transition_project_inner(
        &self,
        project_id: &ProjectId,
        owner_id: &str,
        new_status: ProjectStatus,
    ) -> Result<ResearchProject> {
        let mut project = self.load_project(project_id)?;
        if project.owner_id.0 != owner_id {
            return Err(ScienceError::Ownership);
        }
        project
            .transition_to(new_status)
            .map_err(ScienceError::Invalid)?;
        self.write_project_file(&project)?;
        Ok(project)
    }

    /// Overwrite the research question. Ownership-checked like a transition;
    /// the previous question is preserved in the mutation run record, not
    /// here — the project file states what IS being asked, the run ledger
    /// states what changed and when.
    pub(super) fn update_question_inner(
        &self,
        project_id: &ProjectId,
        owner_id: &str,
        research_question: String,
    ) -> Result<ResearchProject> {
        let mut project = self.load_project(project_id)?;
        if project.owner_id.0 != owner_id {
            return Err(ScienceError::Ownership);
        }
        project.research_question = research_question;
        self.write_project_file(&project)?;
        Ok(project)
    }

    /// Propose a claim node in the evidence graph.
    ///
    /// Two records change together (the claim file and `graph.json`). The
    /// claim file is written **first** because the graph node is fully
    /// derivable from it, so a crash between the two writes is repairable
    /// forward by [`ProjectStore::recover_project`]; the reverse order would
    /// leave a graph node no record can explain.
    pub fn propose_claim(
        &self,
        project_id: &ProjectId,
        owner_id: &str,
        statement: impl Into<String>,
        proposed_by: impl Into<String>,
    ) -> Result<Claim> {
        let _guard = self.write_guard()?;
        self.propose_claim_inner(project_id, owner_id, statement, proposed_by)
    }

    /// Caller must hold the write guard.
    pub(super) fn propose_claim_inner(
        &self,
        project_id: &ProjectId,
        owner_id: &str,
        statement: impl Into<String>,
        proposed_by: impl Into<String>,
    ) -> Result<Claim> {
        self.gates.require(ScienceFeature::ClaimLifecycle)?;
        let project = self.load_project(project_id)?;
        if project.owner_id.0 != owner_id {
            return Err(ScienceError::Ownership);
        }
        let claim_id = format!("claim-{}", Uuid::now_v7());
        let node_id = NodeId(format!("node-{}", claim_id));
        let mut claim = Claim::new(
            claim_id.clone(),
            project_id.clone(),
            statement.into(),
            proposed_by.into(),
        );
        claim.evidence_node_id = Some(node_id.clone());

        let mut graph = self.load_graph(project_id)?;
        let node = EvidenceNode {
            node_id: node_id.clone(),
            kind: NodeKind::Claim,
            project_id: project_id.clone(),
            label: claim.statement.clone(),
            artifact_sha256: None,
            run_id: None,
            created_by: claim.proposed_by.clone(),
            created_at: Utc::now(),
            metadata: BTreeMap::new(),
        };
        graph.add_node(node).map_err(ScienceError::Invalid)?;
        // Claim record first, then the graph node derived from it.
        self.write_claim_file(&claim)?;
        self.write_graph_file(&graph)?;
        Ok(claim)
    }

    pub fn load_claim(&self, project_id: &ProjectId, claim_id: &str) -> Result<Claim> {
        self.gates.require(ScienceFeature::ClaimLifecycle)?;
        let path = Self::claim_record(project_id, claim_id)?;
        self.read_confined(&path)?
            .ok_or_else(|| ScienceError::Invalid(format!("claim not found: {claim_id}")))
    }

    pub fn list_claims(&self, project_id: &ProjectId) -> Result<Vec<Claim>> {
        self.gates.require(ScienceFeature::ClaimLifecycle)?;
        let dir = Self::project_relative(project_id)?.join("claims");
        let mut out: Vec<Claim> = Vec::new();
        for name in self.list_confined(&dir)? {
            let path = dir.join(name);
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let claim: Claim = self.read_confined(&path)?.ok_or_else(|| {
                    ScienceError::Invalid("claim record disappeared during listing".into())
                })?;
                if claim.project_id != *project_id {
                    return Err(ScienceError::Ownership);
                }
                out.push(claim);
            }
        }
        out.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));
        Ok(out)
    }

    /// The evidence-graph node id for an artifact digest.
    ///
    /// The identity uses the **full** digest. A truncated identity would make
    /// two distinct artifacts that happen to share a prefix collapse onto one
    /// evidence node, silently re-pointing every citation of one at the other.
    pub fn artifact_node_id(sha256: &str) -> NodeId {
        NodeId(format!("art-{sha256}"))
    }

    /// Attach a SourceArtifact node and Supports edge to a claim.
    ///
    /// Fail closed on: a non-canonical digest (must be exactly 64 lowercase
    /// hex), a digest that is not registered in this project, a digest
    /// registered to a different project, and any graph mutation that would
    /// leave a dangling endpoint, self-edge, or derivation cycle.
    ///
    /// Two records change together. `graph.json` is written **first** because
    /// the claim's status change is derivable from the edge that lands there,
    /// so a crash between the two writes is repairable forward by
    /// [`ProjectStore::recover_project`]; the reverse order would claim
    /// evidence that no edge backs.
    pub fn attach_evidence(
        &self,
        project_id: &ProjectId,
        owner_id: &str,
        claim_id: &str,
        artifact_sha256: impl Into<String>,
        label: impl Into<String>,
        run_id: Option<String>,
    ) -> Result<(Claim, EvidenceGraph)> {
        let _guard = self.write_guard()?;
        self.attach_evidence_inner(
            project_id,
            owner_id,
            claim_id,
            artifact_sha256,
            label,
            run_id,
        )
    }

    /// Caller must hold the write guard.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn attach_evidence_inner(
        &self,
        project_id: &ProjectId,
        owner_id: &str,
        claim_id: &str,
        artifact_sha256: impl Into<String>,
        label: impl Into<String>,
        run_id: Option<String>,
    ) -> Result<(Claim, EvidenceGraph)> {
        self.gates.require(ScienceFeature::EvidenceGraph)?;
        self.gates.require(ScienceFeature::ClaimLifecycle)?;
        let project = self.load_project(project_id)?;
        if project.owner_id.0 != owner_id {
            return Err(ScienceError::Ownership);
        }
        let sha = artifact_sha256.into();
        validate_sha256_hex(&sha).map_err(ScienceError::Invalid)?;
        let registered = self.require_registered_artifact(project_id, &sha)?;
        let mut claim = self.load_claim(project_id, claim_id)?;
        if claim.project_id != *project_id {
            return Err(ScienceError::Invalid(format!(
                "claim {claim_id} belongs to project {}, not {}",
                claim.project_id.0, project_id.0
            )));
        }
        let claim_node = claim
            .evidence_node_id
            .clone()
            .ok_or_else(|| ScienceError::Invalid("claim missing evidence node".into()))?;

        let mut graph = self.load_graph(project_id)?;
        if !graph.nodes.contains_key(&claim_node) {
            return Err(ScienceError::Invalid(format!(
                "claim node {} is not in the evidence graph; run recover_project first",
                claim_node.0
            )));
        }
        let art_node_id = Self::artifact_node_id(&sha);
        // A graph written before digests were canonical identifies the same
        // artifact under a truncated node id. Refuse rather than silently
        // creating a second node for one artifact.
        if let Some(stale) = graph.nodes.values().find(|node| {
            node.artifact_sha256.as_deref() == Some(sha.as_str()) && node.node_id != art_node_id
        }) {
            return Err(ScienceError::Invalid(format!(
                "artifact {sha} already appears as legacy node {}; migrate the graph to full-digest node ids",
                stale.node_id.0
            )));
        }
        if !graph.nodes.contains_key(&art_node_id) {
            let art_node = EvidenceNode {
                node_id: art_node_id.clone(),
                kind: NodeKind::SourceArtifact,
                project_id: project_id.clone(),
                label: label.into(),
                artifact_sha256: Some(sha.clone()),
                run_id: run_id.clone().or_else(|| registered.run_id.clone()),
                created_by: owner_id.to_string(),
                created_at: Utc::now(),
                metadata: BTreeMap::new(),
            };
            graph.add_node(art_node).map_err(ScienceError::Invalid)?;
        }

        let edge = EvidenceEdge {
            source: art_node_id,
            target: claim_node,
            relation: EdgeKind::Supports,
            actor: owner_id.to_string(),
            timestamp: Utc::now(),
            run_id: run_id.unwrap_or_else(|| "none".into()),
            supporting_artifact_sha256: sha,
            confidence_kind: "high".into(),
        };
        graph.add_edge(edge).map_err(ScienceError::Invalid)?;
        self.write_graph_file(&graph)?;

        if claim.status == ClaimStatus::Proposed {
            claim
                .transition_to(ClaimStatus::EvidenceAttached)
                .map_err(ScienceError::Invalid)?;
        }
        self.write_claim_file(&claim)?;
        Ok((claim, graph))
    }

    /// Repair a project whose multi-file mutation was interrupted.
    ///
    /// `propose_claim` and `attach_evidence` each touch two records. They are
    /// not journalled — full transactionality across the record tree is a
    /// larger change — so instead each writes the *derivable-from* record
    /// first, which makes every crash point repairable forward from what did
    /// land:
    ///
    /// - claim record present, graph node missing → rebuild the node from the
    ///   claim (a `propose_claim` that stopped after its first write);
    /// - supporting edge present, claim still `Proposed` → advance the claim
    ///   (an `attach_evidence` that stopped after its first write);
    /// - graph claim node with no claim record → drop the node and its edges
    ///   (nothing can explain it, so it must not keep supporting anything);
    /// - edges left dangling by the above → drop;
    /// - temp files from a write that never renamed → remove.
    ///
    /// Idempotent: running it on a healthy project reports no repairs.
    pub fn recover_project(&self, project_id: &ProjectId) -> Result<ProjectRecoveryReport> {
        self.gates.require(ScienceFeature::ResearchProject)?;
        let _guard = self.write_guard()?;
        let mut report = ProjectRecoveryReport::default();

        let dir = Self::project_relative(project_id)?;
        if self
            .confined()?
            .read_optional(&dir.join("project.json"))?
            .is_none()
        {
            return Err(ScienceError::Invalid(format!(
                "project not found: {}",
                project_id.0
            )));
        }
        report.stale_temp_files_removed = self.sweep_temp_files(&dir)?
            + self.sweep_temp_files(&dir.join("claims"))?
            + self.sweep_temp_files(&dir.join("reviews"))?;

        let claims = self.list_claims(project_id)?;
        let mut graph = self.load_graph(project_id)?;
        let mut graph_changed = false;

        // 1. Graph claim nodes with no claim record behind them.
        let known_nodes: std::collections::BTreeSet<NodeId> = claims
            .iter()
            .filter_map(|claim| claim.evidence_node_id.clone())
            .collect();
        let orphans: Vec<NodeId> = graph
            .nodes
            .values()
            .filter(|node| {
                matches!(node.kind, NodeKind::Claim) && !known_nodes.contains(&node.node_id)
            })
            .map(|node| node.node_id.clone())
            .collect();
        for node_id in orphans {
            graph.nodes.remove(&node_id);
            report.orphan_nodes_removed.push(node_id.0);
            graph_changed = true;
        }

        // 2. Claim records whose graph node never landed.
        for claim in &claims {
            let Some(node_id) = claim.evidence_node_id.clone() else {
                continue;
            };
            if graph.nodes.contains_key(&node_id) {
                continue;
            }
            graph
                .add_node(EvidenceNode {
                    node_id,
                    kind: NodeKind::Claim,
                    project_id: project_id.clone(),
                    label: claim.statement.clone(),
                    artifact_sha256: None,
                    run_id: None,
                    created_by: claim.proposed_by.clone(),
                    created_at: claim.created_at,
                    metadata: BTreeMap::new(),
                })
                .map_err(ScienceError::Invalid)?;
            report.claim_nodes_restored.push(claim.claim_id.clone());
            graph_changed = true;
        }

        // 3. Edges whose endpoints no longer exist.
        let before = graph.edges.len();
        graph.edges.retain(|edge| {
            graph.nodes.contains_key(&edge.source) && graph.nodes.contains_key(&edge.target)
        });
        report.orphan_edges_removed = before - graph.edges.len();
        graph_changed |= report.orphan_edges_removed > 0;

        if graph_changed {
            self.write_graph_file(&graph)?;
        }

        // 4. Claims that already have supporting evidence but never advanced.
        for claim in claims {
            if claim.status != ClaimStatus::Proposed {
                continue;
            }
            let Some(node_id) = claim.evidence_node_id.clone() else {
                continue;
            };
            let supported = graph
                .edges
                .iter()
                .any(|edge| edge.target == node_id && edge.relation == EdgeKind::Supports);
            if !supported {
                continue;
            }
            let mut claim = claim;
            claim
                .transition_to(ClaimStatus::EvidenceAttached)
                .map_err(ScienceError::Invalid)?;
            self.write_claim_file(&claim)?;
            report.claims_advanced.push(claim.claim_id);
        }

        Ok(report)
    }

    /// Remove temp files left by a write that died before its rename.
    fn sweep_temp_files(&self, dir: &Path) -> Result<usize> {
        let mut removed = 0;
        for name in self.list_confined(dir)? {
            let Some(name) = name.to_str() else { continue };
            if name.starts_with(TEMP_PREFIX)
                && name.ends_with(".tmp")
                && self.confined()?.remove_file(&dir.join(name))?
            {
                removed += 1;
            }
        }
        Ok(removed)
    }

    pub fn load_bundle(&self, project_id: &ProjectId) -> Result<ProjectBundle> {
        Ok(ProjectBundle {
            project: self.load_project(project_id)?,
            graph: self.load_graph(project_id)?,
            claims: self.list_claims(project_id)?,
        })
    }

    /// Load a complete project bundle for its owner.
    ///
    /// The aggregate root is ownership-checked before any graph or claim bytes
    /// are returned. Missing projects and foreign projects intentionally share
    /// the same [`ScienceError::Ownership`] result.
    pub fn load_bundle_for_owner(
        &self,
        project_id: &ProjectId,
        owner_id: &str,
    ) -> Result<ProjectBundle> {
        let project = self.load_owned_project(project_id, owner_id)?;
        Ok(ProjectBundle {
            project,
            graph: self.load_graph(project_id)?,
            claims: self.list_claims(project_id)?,
        })
    }

    pub fn list_projects(&self) -> Result<Vec<ResearchProject>> {
        self.gates.require(ScienceFeature::ResearchProject)?;
        let mut out: Vec<ResearchProject> = Vec::new();
        for name in self.list_confined(Path::new("projects"))? {
            let Some(project_name) = name.to_str() else {
                continue;
            };
            if validate_project_id(project_name).is_err() {
                continue;
            }
            let pj = PathBuf::from("projects")
                .join(project_name)
                .join("project.json");
            if let Some(project) = self.read_confined::<ResearchProject>(&pj)? {
                if project.project_id.0 != project_name {
                    return Err(ScienceError::Ownership);
                }
                out.push(project);
            }
        }
        out.sort_by(|a, b| a.project_id.0.cmp(&b.project_id.0));
        Ok(out)
    }

    /// List only projects owned by `owner_id`.
    ///
    /// Foreign projects are filtered out rather than reported as errors, so
    /// an empty result does not reveal whether the store is empty or contains
    /// projects owned by somebody else. Unreadable records are omitted because
    /// their ownership cannot be proven.
    pub fn list_projects_for_owner(&self, owner_id: &str) -> Result<Vec<ResearchProject>> {
        self.gates.require(ScienceFeature::ResearchProject)?;
        if owner_id.is_empty() {
            return Err(ScienceError::Ownership);
        }
        let mut out: Vec<ResearchProject> = Vec::new();
        for name in self.list_confined(Path::new("projects"))? {
            let Some(project_name) = name.to_str() else {
                continue;
            };
            if validate_project_id(project_name).is_err() {
                continue;
            }
            let project_path = PathBuf::from("projects")
                .join(project_name)
                .join("project.json");
            let Ok(Some(project)): Result<Option<ResearchProject>> =
                self.read_confined(&project_path)
            else {
                // A record whose owner cannot be established must never be
                // exposed through an owner-scoped listing.
                continue;
            };
            if project.owner_id.0 != owner_id {
                continue;
            }
            if validate_project_id(&project.project_id.0).is_err()
                || project_name != project.project_id.0
            {
                continue;
            }
            out.push(project);
        }
        out.sort_by(|a, b| a.project_id.0.cmp(&b.project_id.0));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[cfg(unix)]
    const PROJECT_LOCK_CHILD_ROOT_ENV: &str = "LUMEN_TEST_PROJECT_LOCK_CHILD_ROOT";
    #[cfg(unix)]
    const PROJECT_LOCK_CHILD_READY_ENV: &str = "LUMEN_TEST_PROJECT_LOCK_CHILD_READY";
    #[cfg(unix)]
    const PROJECT_LOCK_CHILD_PROJECT_ENV: &str = "LUMEN_TEST_PROJECT_LOCK_CHILD_PROJECT";
    #[cfg(unix)]
    const PROJECT_LOCK_CHILD_STARTED_ENV: &str = "LUMEN_TEST_PROJECT_LOCK_CHILD_STARTED";
    #[cfg(unix)]
    const PROJECT_LOCK_CHILD_FINISHED_ENV: &str = "LUMEN_TEST_PROJECT_LOCK_CHILD_FINISHED";

    /// Subprocess-only helper for
    /// `cross_process_writer_waits_until_crashed_holder_releases_lock`.
    ///
    /// A normal test-harness invocation has no root environment variable and
    /// returns immediately. The parent launches this exact test in a fresh
    /// process, waits until the descriptor lock is held, then terminates it.
    #[cfg(unix)]
    #[test]
    fn project_write_lock_child_helper() {
        let Some(root) = std::env::var_os(PROJECT_LOCK_CHILD_ROOT_ENV) else {
            return;
        };
        let ready = std::env::var_os(PROJECT_LOCK_CHILD_READY_ENV)
            .expect("parent must provide the child-ready marker");
        let store = ProjectStore::new(PathBuf::from(root));
        let _guard = store
            .write_guard()
            .expect("child must acquire the project-store lock");
        fs::write(ready, b"locked").expect("child must publish its ready marker");
        loop {
            std::thread::park();
        }
    }

    /// Subprocess-only ProjectStore writer used to prove that the guarded
    /// revision seam excludes a mutation from another Lumen process.
    #[cfg(unix)]
    #[test]
    fn project_mutation_child_helper() {
        let Some(root) = std::env::var_os(PROJECT_LOCK_CHILD_ROOT_ENV) else {
            return;
        };
        let project_id = ProjectId(
            std::env::var(PROJECT_LOCK_CHILD_PROJECT_ENV)
                .expect("parent must provide the child project id"),
        );
        let started = std::env::var_os(PROJECT_LOCK_CHILD_STARTED_ENV)
            .expect("parent must provide the child-started marker");
        let finished = std::env::var_os(PROJECT_LOCK_CHILD_FINISHED_ENV)
            .expect("parent must provide the child-finished marker");
        let store = ProjectStore::new(PathBuf::from(root));
        fs::write(started, b"attempting").expect("child must publish its attempted mutation");
        store
            .transition_project(&project_id, "owner", ProjectStatus::Planned)
            .expect("child project mutation must complete after the parent releases its guard");
        fs::write(finished, b"finished").expect("child must publish mutation completion");
    }

    #[cfg(unix)]
    #[test]
    fn guarded_revision_recheck_blocks_cross_process_project_mutation() {
        use std::{
            process::{Command, Stdio},
            time::{Duration, Instant},
        };

        let dir = tempdir().unwrap();
        let root = dir.path().join("store");
        let started = dir.path().join("child-started");
        let finished = dir.path().join("child-finished");
        fs::create_dir(&root).unwrap();
        let store = ProjectStore::new(&root);
        let project = store
            .create_project("owner", "Revision guard", "Is the snapshot stable?")
            .unwrap();

        let mut child = store
            .with_owned_project_revision_guarded(
                &project.project_id,
                "owner",
                |rechecked_project, revision, _guard| {
                    assert_eq!(rechecked_project.project_id, project.project_id);
                    assert_eq!(revision, store.project_revision(&project.project_id)?);

                    let mut child = Command::new(std::env::current_exe()?)
                        .arg("--exact")
                        .arg("project::store::tests::project_mutation_child_helper")
                        .arg("--nocapture")
                        .env(PROJECT_LOCK_CHILD_ROOT_ENV, &root)
                        .env(PROJECT_LOCK_CHILD_PROJECT_ENV, &project.project_id.0)
                        .env(PROJECT_LOCK_CHILD_STARTED_ENV, &started)
                        .env(PROJECT_LOCK_CHILD_FINISHED_ENV, &finished)
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()?;

                    let started_deadline = Instant::now() + Duration::from_secs(10);
                    loop {
                        if started.is_file() {
                            break;
                        }
                        if let Some(status) = child.try_wait()? {
                            return Err(ScienceError::Invalid(format!(
                                "project-mutation child exited before attempting its write: {status}"
                            )));
                        }
                        if Instant::now() >= started_deadline {
                            let _ = child.kill();
                            let _ = child.wait();
                            return Err(ScienceError::Invalid(
                                "project-mutation child did not start within 10 seconds".into(),
                            ));
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }

                    std::thread::sleep(Duration::from_millis(300));
                    if finished.exists() {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(ScienceError::Invalid(
                            "cross-process mutation crossed the guarded revision seam".into(),
                        ));
                    }
                    if let Some(status) = child.try_wait()? {
                        return Err(ScienceError::Invalid(format!(
                            "project-mutation child exited while the revision guard was held: {status}"
                        )));
                    }
                    Ok(child)
                },
            )
            .unwrap();

        let finish_deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= finish_deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("child mutation remained blocked after the revision guard was dropped");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(
            status.success(),
            "child mutation failed after the revision guard was dropped: {status}"
        );
        assert!(finished.is_file());
        assert_eq!(
            store.load_project(&project.project_id).unwrap().status,
            ProjectStatus::Planned
        );
    }

    #[cfg(unix)]
    #[test]
    fn cross_process_writer_waits_until_crashed_holder_releases_lock() {
        use std::{
            process::{Command, Stdio},
            sync::mpsc::{self, RecvTimeoutError},
            time::{Duration, Instant},
        };

        let dir = tempdir().unwrap();
        let root = dir.path().join("store");
        let ready = dir.path().join("child-ready");
        fs::create_dir(&root).unwrap();
        let store = ProjectStore::new(&root);
        let project = store
            .create_project("owner", "Cross-process lock", "Does it block?")
            .unwrap();

        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("project::store::tests::project_write_lock_child_helper")
            .arg("--nocapture")
            .env(PROJECT_LOCK_CHILD_ROOT_ENV, &root)
            .env(PROJECT_LOCK_CHILD_READY_ENV, &ready)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let ready_deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if ready.is_file() {
                break;
            }
            if let Some(status) = child.try_wait().unwrap() {
                panic!("lock-holder child exited before acquiring the lock: {status}");
            }
            if Instant::now() >= ready_deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("lock-holder child did not acquire the lock within 10 seconds");
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        let writer = ProjectStore::new(&root);
        let project_id = project.project_id.clone();
        let (finished_tx, finished_rx) = mpsc::channel();
        let writer_thread = std::thread::spawn(move || {
            let result = writer.transition_project(&project_id, "owner", ProjectStatus::Planned);
            finished_tx.send(result).unwrap();
        });

        assert!(
            matches!(
                finished_rx.recv_timeout(Duration::from_millis(300)),
                Err(RecvTimeoutError::Timeout)
            ),
            "a second process mutated the project while the child held the root lock"
        );

        child.kill().unwrap();
        let status = child.wait().unwrap();
        assert!(
            !status.success(),
            "the crash-cut child unexpectedly succeeded"
        );

        let transitioned = finished_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("writer remained blocked after the crashed holder released its descriptors")
            .expect("writer failed after the crashed holder released the lock");
        assert_eq!(transitioned.status, ProjectStatus::Planned);
        writer_thread.join().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn project_write_lock_symlink_fails_closed_without_project_output() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let root = dir.path().join("store");
        fs::create_dir(&root).unwrap();
        let outside = dir.path().join("outside-lock");
        fs::write(&outside, b"").unwrap();
        symlink(
            &outside,
            root.join(super::super::capability::PROJECT_WRITE_LOCK_FILE),
        )
        .unwrap();

        let store = ProjectStore::new(&root);
        assert!(store.create_project("owner", "title", "question").is_err());
        assert!(!root.join("projects").exists());
    }

    #[cfg(unix)]
    #[test]
    fn project_write_lock_non_file_path_fails_closed_without_project_output() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("store");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join(super::super::capability::PROJECT_WRITE_LOCK_FILE)).unwrap();

        let store = ProjectStore::new(&root);
        assert!(store.create_project("owner", "title", "question").is_err());
        assert!(!root.join("projects").exists());
    }

    #[cfg(unix)]
    #[test]
    fn project_write_lock_unsafe_permissions_fail_closed() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempdir().unwrap();
        let root = dir.path().join("store");
        fs::create_dir(&root).unwrap();
        let lock = root.join(super::super::capability::PROJECT_WRITE_LOCK_FILE);
        fs::write(&lock, b"").unwrap();
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o660)).unwrap();

        let store = ProjectStore::new(&root);
        assert!(store.write_guard().is_err());
        assert!(!root.join("projects").exists());
    }

    #[cfg(unix)]
    #[test]
    fn project_write_lock_hard_link_fails_closed() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempdir().unwrap();
        let root = dir.path().join("store");
        fs::create_dir(&root).unwrap();
        let outside = dir.path().join("outside-lock");
        fs::write(&outside, b"").unwrap();
        fs::set_permissions(&outside, fs::Permissions::from_mode(0o600)).unwrap();
        fs::hard_link(
            &outside,
            root.join(super::super::capability::PROJECT_WRITE_LOCK_FILE),
        )
        .unwrap();

        let store = ProjectStore::new(&root);
        assert!(store.write_guard().is_err());
        assert!(!root.join("projects").exists());
    }

    #[cfg(unix)]
    #[test]
    fn project_write_lock_rejects_group_or_world_writable_root() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempdir().unwrap();
        let root = dir.path().join("store");
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o777)).unwrap();

        let store = ProjectStore::new(&root);
        assert!(store.write_guard().is_err());
        assert!(
            !root
                .join(super::super::capability::PROJECT_WRITE_LOCK_FILE)
                .exists()
        );
        assert!(!root.join("projects").exists());
    }

    #[test]
    fn create_claim_attach_evidence_roundtrip() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store
            .create_project("owner-1", "Demo", "Does EcoRI cut?")
            .unwrap();
        assert_eq!(p.status, ProjectStatus::Draft);

        let claim = store
            .propose_claim(&p.project_id, "owner-1", "EcoRI site present", "scientist")
            .unwrap();
        assert_eq!(claim.status, ClaimStatus::Proposed);

        let sha = "a".repeat(64);
        // Evidence may only cite a registered artifact, so register it first.
        store
            .register_artifact(
                &p.project_id,
                "owner-1",
                sha.clone(),
                "seq analysis",
                Some("run-1".into()),
            )
            .unwrap();
        let (claim2, graph) = store
            .attach_evidence(
                &p.project_id,
                "owner-1",
                &claim.claim_id,
                sha,
                "seq analysis",
                Some("run-1".into()),
            )
            .unwrap();
        assert_eq!(claim2.status, ClaimStatus::EvidenceAttached);
        assert!(graph.nodes.len() >= 2);
        assert!(!graph.edges.is_empty());

        // ownership fail closed
        assert!(matches!(
            store.propose_claim(&p.project_id, "other", "x", "x"),
            Err(ScienceError::Ownership)
        ));

        // bad hash fail closed
        assert!(
            store
                .attach_evidence(&p.project_id, "owner-1", &claim.claim_id, "nope", "x", None)
                .is_err()
        );
    }

    #[test]
    fn owner_scoped_bundle_hides_foreign_and_missing_projects() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let project = store
            .create_project("owner-1", "Private project", "Private question")
            .unwrap();
        store
            .propose_claim(&project.project_id, "owner-1", "private claim", "scientist")
            .unwrap();

        let owned = store
            .load_bundle_for_owner(&project.project_id, "owner-1")
            .unwrap();
        assert_eq!(owned.project.project_id, project.project_id);
        assert_eq!(owned.claims.len(), 1);

        assert!(matches!(
            store.load_bundle_for_owner(&project.project_id, "owner-2"),
            Err(ScienceError::Ownership)
        ));
        assert!(matches!(
            store.load_bundle_for_owner(&ProjectId("missing-project".into()), "owner-2"),
            Err(ScienceError::Ownership)
        ));
        assert!(matches!(
            store.load_bundle_for_owner(&project.project_id, ""),
            Err(ScienceError::Ownership)
        ));
    }

    #[test]
    fn owner_scoped_list_filters_foreign_projects_without_disclosure() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let owned = store.create_project("owner-1", "Owned", "Q1").unwrap();
        let foreign = store.create_project("owner-2", "Foreign", "Q2").unwrap();

        let owner_projects = store.list_projects_for_owner("owner-1").unwrap();
        assert_eq!(owner_projects.len(), 1);
        assert_eq!(owner_projects[0].project_id, owned.project_id);
        assert!(
            owner_projects
                .iter()
                .all(|project| project.project_id != foreign.project_id)
        );
        assert!(
            store
                .list_projects_for_owner("unknown-owner")
                .unwrap()
                .is_empty(),
            "an empty owner scope must not disclose foreign projects"
        );
        assert!(matches!(
            store.list_projects_for_owner(""),
            Err(ScienceError::Ownership)
        ));
    }

    #[test]
    fn evidence_query_ownership_assertion_rejects_foreign_owner() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let project = store
            .create_project("owner-1", "Evidence", "Private evidence?")
            .unwrap();
        store
            .propose_claim(
                &project.project_id,
                "owner-1",
                "private evidence claim",
                "scientist",
            )
            .unwrap();

        // Evidence-query adapters must call this assertion before loading a
        // claim or graph. Foreign and missing projects have the same terminal
        // error, so the assertion cannot be used as an existence oracle.
        assert!(
            store
                .assert_project_owner(&project.project_id, "owner-1")
                .is_ok()
        );
        assert!(matches!(
            store.assert_project_owner(&project.project_id, "owner-2"),
            Err(ScienceError::Ownership)
        ));
        assert!(matches!(
            store.assert_project_owner(&ProjectId("missing-project".into()), "owner-2"),
            Err(ScienceError::Ownership)
        ));
        assert!(matches!(
            store.assert_project_owner(&project.project_id, ""),
            Err(ScienceError::Ownership)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn owner_scoped_reads_never_follow_project_record_symlinks() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path().join("store"));
        let outside = ProjectStore::new(dir.path().join("outside"));
        let foreign = outside
            .create_project("owner-1", "Outside", "Must stay outside")
            .unwrap();
        let projects = store.root.join("projects");
        fs::create_dir_all(&projects).unwrap();
        symlink(
            outside.project_dir(&foreign.project_id),
            projects.join(&foreign.project_id.0),
        )
        .unwrap();

        assert!(matches!(
            store.load_bundle_for_owner(&foreign.project_id, "owner-1"),
            Err(ScienceError::Ownership)
        ));
        assert!(
            store.list_projects_for_owner("owner-1").unwrap().is_empty(),
            "owner-scoped list followed a symlinked project directory"
        );
    }

    #[cfg(unix)]
    #[test]
    fn product_writes_refuse_symlinked_parent_without_outside_bytes() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let root = dir.path().join("store");
        let outside = dir.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        let store = ProjectStore::new(&root);
        symlink(&outside, root.join("projects")).unwrap();

        assert!(
            store
                .create_project("owner-1", "Escaped?", "Must not escape")
                .is_err(),
            "a symlinked projects directory accepted a product write"
        );
        assert_eq!(
            fs::read_dir(&outside).unwrap().count(),
            0,
            "the rejected project write created bytes outside the pinned root"
        );
    }

    #[cfg(unix)]
    #[test]
    fn pinned_root_survives_ancestor_path_swap_without_redirection() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let root = dir.path().join("store");
        let retained = dir.path().join("retained-store");
        let outside = dir.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        let store = ProjectStore::new(&root);

        fs::rename(&root, &retained).unwrap();
        symlink(&outside, &root).unwrap();

        let project = store
            .create_project("owner-1", "Pinned", "Where do the bytes land?")
            .unwrap();
        assert!(
            outside.read_dir().unwrap().next().is_none(),
            "a root pathname swap redirected bytes outside the retained capability"
        );
        assert!(
            retained
                .join("projects")
                .join(&project.project_id.0)
                .join("project.json")
                .is_file(),
            "the retained directory capability did not receive the project record"
        );
        assert_eq!(
            store.load_project(&project.project_id).unwrap().project_id,
            project.project_id,
            "reads did not remain bound to the same retained root"
        );
    }

    #[cfg(unix)]
    #[test]
    fn confined_constructor_proves_root_identity_inside_workspace() {
        use std::os::unix::fs::symlink;

        let workspace = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let root = workspace.path().join("store");
        let retained = workspace.path().join("retained-store");
        let store = ProjectStore::new_confined(&root, workspace.path()).unwrap();

        fs::rename(&root, &retained).unwrap();
        symlink(outside.path(), &root).unwrap();
        let project = store
            .create_project("owner-1", "Pinned", "Retained capability?")
            .unwrap();

        assert!(
            outside.path().read_dir().unwrap().next().is_none(),
            "a confined store followed its replaced root pathname"
        );
        assert!(
            retained
                .join("projects")
                .join(project.project_id.0)
                .join("project.json")
                .is_file()
        );
    }

    #[cfg(unix)]
    #[test]
    fn confined_constructor_rejects_outside_and_symlink_roots() {
        use std::os::unix::fs::symlink;

        let workspace = tempdir().unwrap();
        let outside = tempdir().unwrap();
        assert!(ProjectStore::new_confined(outside.path(), workspace.path()).is_err());

        let linked = workspace.path().join("linked-store");
        symlink(outside.path(), &linked).unwrap();
        assert!(ProjectStore::new_confined(&linked, workspace.path()).is_err());
        assert!(
            outside.path().read_dir().unwrap().next().is_none(),
            "a rejected confined constructor wrote outside its workspace"
        );
    }

    #[cfg(unix)]
    #[test]
    fn final_record_symlink_is_replaced_without_touching_target() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let root = dir.path().join("store");
        let outside = dir.path().join("outside.json");
        fs::write(&outside, b"outside-sentinel").unwrap();
        let store = ProjectStore::new(&root);
        let mut project = store
            .create_project("owner-1", "Before", "Is publication confined?")
            .unwrap();
        let project_path = store.project_dir(&project.project_id).join("project.json");
        fs::remove_file(&project_path).unwrap();
        symlink(&outside, &project_path).unwrap();

        project.title = "After".into();
        store.save_project(&project).unwrap();

        assert_eq!(
            fs::read(&outside).unwrap(),
            b"outside-sentinel",
            "atomic publication followed a final-record symlink"
        );
        assert_eq!(
            store.load_project(&project.project_id).unwrap().title,
            "After"
        );
        assert!(
            !fs::symlink_metadata(project_path)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the safe publication did not replace the symlink entry"
        );
    }

    #[cfg(unix)]
    #[test]
    fn operation_ledger_parent_symlink_fails_before_mutation() {
        use crate::project::{MutationRequest, ProjectMutation};
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let root = dir.path().join("store");
        let outside = dir.path().join("outside-operations");
        fs::create_dir_all(&outside).unwrap();
        let store = ProjectStore::new(&root);
        let project = store
            .create_project("owner-1", "Before", "Original question")
            .unwrap();
        let revision = store.project_revision(&project.project_id).unwrap();
        symlink(&outside, root.join("operations")).unwrap();

        let request = MutationRequest {
            operation_id: "op-symlink-ledger".into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            expected_revision: Some(revision),
            mutation: ProjectMutation::QuestionUpdate {
                project_id: project.project_id.clone(),
                research_question: "Redirected question".into(),
            },
        };
        assert!(
            store.apply_mutation(&request).is_err(),
            "a symlinked operation ledger accepted a mutation"
        );
        assert_eq!(
            store
                .load_project(&project.project_id)
                .unwrap()
                .research_question,
            "Original question",
            "operation-ledger confinement failed after mutating aggregate state"
        );
        assert_eq!(
            fs::read_dir(&outside).unwrap().count(),
            0,
            "the rejected operation wrote outside the retained store"
        );
    }

    /// Set up a project with one claim and one registered artifact.
    fn seeded(dir: &std::path::Path) -> (ProjectStore, ResearchProject, Claim, String) {
        let store = ProjectStore::new(dir);
        let project = store.create_project("o", "T", "Q").unwrap();
        let claim = store
            .propose_claim(&project.project_id, "o", "statement", "sci")
            .unwrap();
        let sha = "a".repeat(64);
        store
            .register_artifact(&project.project_id, "o", sha.clone(), "art", None)
            .unwrap();
        (store, project, claim, sha)
    }

    // ── Defect A regression: digest identity + registry ────────────

    /// The defect: node identity was `art-{&sha[..16]}`, so two distinct
    /// artifacts sharing a 16-character prefix collapsed onto one node.
    #[test]
    fn distinct_digests_sharing_a_prefix_get_distinct_nodes() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.create_project("o", "T", "Q").unwrap();
        let claim = store.propose_claim(&p.project_id, "o", "s", "sci").unwrap();

        let prefix = "0123456789abcdef";
        let first = format!("{prefix}{}", "1".repeat(48));
        let second = format!("{prefix}{}", "2".repeat(48));
        for sha in [&first, &second] {
            store
                .register_artifact(&p.project_id, "o", sha.clone(), "art", None)
                .unwrap();
            store
                .attach_evidence(&p.project_id, "o", &claim.claim_id, sha.clone(), "l", None)
                .unwrap();
        }

        let graph = store.load_graph(&p.project_id).unwrap();
        assert!(
            graph
                .nodes
                .contains_key(&ProjectStore::artifact_node_id(&first))
        );
        assert!(
            graph
                .nodes
                .contains_key(&ProjectStore::artifact_node_id(&second))
        );
        // claim node + two distinct artifact nodes
        assert_eq!(
            graph.nodes.len(),
            3,
            "prefix collision collapsed two artifacts"
        );
        assert_eq!(graph.edges.len(), 2);
    }

    #[test]
    fn attach_evidence_rejects_non_canonical_digests() {
        let dir = tempdir().unwrap();
        let (store, p, claim, _) = seeded(dir.path());
        for bad in [
            "a".repeat(16), // the old minimum — now rejected
            "a".repeat(63), // too short
            "a".repeat(65), // too long
            "A".repeat(64), // uppercase is not normalised
            "g".repeat(64), // non-hex
            format!("sha256:{}", "a".repeat(57)),
            String::new(),
        ] {
            let error = store
                .attach_evidence(&p.project_id, "o", &claim.claim_id, bad.clone(), "l", None)
                .unwrap_err();
            assert!(
                matches!(&error, ScienceError::Invalid(m) if m.contains("artifact digest")),
                "digest {bad:?} was not rejected as malformed: {error}"
            );
        }
    }

    #[test]
    fn attach_evidence_rejects_unregistered_artifact() {
        let dir = tempdir().unwrap();
        let (store, p, claim, _) = seeded(dir.path());
        let unknown = "b".repeat(64);
        let error = store
            .attach_evidence(&p.project_id, "o", &claim.claim_id, unknown, "l", None)
            .unwrap_err();
        assert!(
            matches!(&error, ScienceError::Invalid(m) if m.contains("not registered")),
            "unexpected: {error}"
        );
    }

    #[test]
    fn attach_evidence_rejects_cross_project_artifact() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let a = store.create_project("o", "A", "Q").unwrap();
        let b = store.create_project("o", "B", "Q").unwrap();
        let claim = store.propose_claim(&b.project_id, "o", "s", "sci").unwrap();
        let sha = "c".repeat(64);
        store
            .register_artifact(&a.project_id, "o", sha.clone(), "art", None)
            .unwrap();

        let error = store
            .attach_evidence(&b.project_id, "o", &claim.claim_id, sha, "l", None)
            .unwrap_err();
        assert!(
            matches!(&error, ScienceError::Invalid(m) if m.contains("cross projects")),
            "unexpected: {error}"
        );
    }

    #[test]
    fn attach_evidence_rejects_unknown_claim_and_foreign_owner() {
        let dir = tempdir().unwrap();
        let (store, p, _claim, sha) = seeded(dir.path());
        assert!(matches!(
            store.attach_evidence(&p.project_id, "intruder", "claim-x", sha.clone(), "l", None),
            Err(ScienceError::Ownership)
        ));
        assert!(
            store
                .attach_evidence(&p.project_id, "o", "claim-does-not-exist", sha, "l", None)
                .is_err()
        );
    }

    #[test]
    fn register_artifact_validates_digest_owner_and_is_idempotent() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.create_project("o", "T", "Q").unwrap();

        assert!(
            store
                .register_artifact(&p.project_id, "o", "a".repeat(16), "l", None)
                .is_err()
        );
        assert!(
            store
                .register_artifact(&p.project_id, "o", "A".repeat(64), "l", None)
                .is_err()
        );
        assert!(matches!(
            store.register_artifact(&p.project_id, "intruder", "a".repeat(64), "l", None),
            Err(ScienceError::Ownership)
        ));

        let sha = "a".repeat(64);
        let first = store
            .register_artifact(&p.project_id, "o", sha.clone(), "l", None)
            .unwrap();
        let again = store
            .register_artifact(&p.project_id, "o", sha.clone(), "other label", None)
            .unwrap();
        assert_eq!(first, again, "re-registration must be idempotent");
        assert_eq!(store.list_artifacts(&p.project_id).unwrap().len(), 1);
    }

    /// A store written before digests were canonical must fail loudly on the
    /// next mutation rather than being silently re-persisted.
    #[test]
    fn legacy_graph_with_truncated_digest_is_rejected_on_save() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.create_project("o", "T", "Q").unwrap();
        let mut graph = store.load_graph(&p.project_id).unwrap();
        graph.nodes.insert(
            NodeId("art-aaaaaaaaaaaaaaaa".into()),
            EvidenceNode {
                node_id: NodeId("art-aaaaaaaaaaaaaaaa".into()),
                kind: NodeKind::SourceArtifact,
                project_id: p.project_id.clone(),
                label: "legacy".into(),
                artifact_sha256: Some("a".repeat(16)),
                run_id: None,
                created_by: "o".into(),
                created_at: Utc::now(),
                metadata: BTreeMap::new(),
            },
        );
        let error = store.save_graph(&graph).unwrap_err();
        assert!(
            matches!(&error, ScienceError::Invalid(m) if m.contains("artifact digest")),
            "unexpected: {error}"
        );
    }

    /// A legacy graph that carries a full digest under a truncated node id is
    /// refused too, rather than growing a second node for one artifact.
    #[test]
    fn legacy_truncated_node_id_blocks_attach() {
        let dir = tempdir().unwrap();
        let (store, p, claim, sha) = seeded(dir.path());
        let mut graph = store.load_graph(&p.project_id).unwrap();
        let legacy = NodeId(format!("art-{}", &sha[..16]));
        graph.nodes.insert(
            legacy.clone(),
            EvidenceNode {
                node_id: legacy,
                kind: NodeKind::SourceArtifact,
                project_id: p.project_id.clone(),
                label: "legacy".into(),
                artifact_sha256: Some(sha.clone()),
                run_id: None,
                created_by: "o".into(),
                created_at: Utc::now(),
                metadata: BTreeMap::new(),
            },
        );
        store.save_graph(&graph).unwrap();

        let error = store
            .attach_evidence(&p.project_id, "o", &claim.claim_id, sha, "l", None)
            .unwrap_err();
        assert!(
            matches!(&error, ScienceError::Invalid(m) if m.contains("legacy node")),
            "unexpected: {error}"
        );
    }

    // ── Defect B regression: durability + crash repair ─────────────

    fn temp_files_under(dir: &std::path::Path) -> Vec<String> {
        let mut out = Vec::new();
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.path().is_dir() {
                out.extend(temp_files_under(&entry.path()));
            } else if name.starts_with(TEMP_PREFIX) || name.ends_with(".tmp") {
                out.push(name);
            }
        }
        out
    }

    /// Overwrite a record behind the store's back, the way a crash mid-mutation
    /// would leave it.
    fn clobber_graph(store: &ProjectStore, project_id: &ProjectId, graph: &EvidenceGraph) {
        let path = store.project_dir(project_id).join("graph.json");
        fs::write(&path, serde_json::to_vec_pretty(graph).unwrap()).unwrap();
    }

    /// The defect: a fixed `path.with_extension("tmp")` temp name meant two
    /// writers of one record raced on a single temp path, and no mutex
    /// serialised the load-modify-save of `graph.json`, so concurrent claims
    /// silently overwrote each other.
    #[test]
    fn concurrent_writers_do_not_lose_updates() {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let setup = ProjectStore::new(&root);
        let project = setup.create_project("o", "T", "Q").unwrap();
        let project_id = project.project_id.clone();

        const THREADS: usize = 4;
        const PER_THREAD: usize = 5;
        std::thread::scope(|scope| {
            for thread in 0..THREADS {
                let root = root.clone();
                let project_id = project_id.clone();
                scope.spawn(move || {
                    // A fresh store per thread, exactly as the ACP handlers
                    // build one per request.
                    let store = ProjectStore::new(&root);
                    for index in 0..PER_THREAD {
                        store
                            .propose_claim(
                                &project_id,
                                "o",
                                format!("claim {thread}/{index}"),
                                "sci",
                            )
                            .unwrap();
                        let sha = format!("{thread:02x}{index:02x}{}", "0".repeat(60));
                        store
                            .register_artifact(&project_id, "o", sha, "art", None)
                            .unwrap();
                    }
                });
            }
        });

        let total = THREADS * PER_THREAD;
        let claims = setup.list_claims(&project_id).unwrap();
        assert_eq!(claims.len(), total, "claim records were lost");
        let graph = setup.load_graph(&project_id).unwrap();
        assert_eq!(graph.nodes.len(), total, "graph nodes were lost");
        for claim in &claims {
            let node = claim.evidence_node_id.clone().unwrap();
            assert!(
                graph.nodes.contains_key(&node),
                "missing node for {}",
                claim.claim_id
            );
        }
        assert_eq!(
            setup.list_artifacts(&project_id).unwrap().len(),
            total,
            "artifact registrations were lost"
        );
        assert!(graph.validate_integrity().is_ok());
        assert!(
            temp_files_under(dir.path()).is_empty(),
            "durable writes left temp files behind: {:?}",
            temp_files_under(dir.path())
        );
    }

    /// Concurrent attach_evidence against one claim: every edge must survive.
    #[test]
    fn concurrent_evidence_attachments_all_land() {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let setup = ProjectStore::new(&root);
        let project = setup.create_project("o", "T", "Q").unwrap();
        let claim = setup
            .propose_claim(&project.project_id, "o", "s", "sci")
            .unwrap();
        let digests: Vec<String> = (0..6)
            .map(|index| format!("{index:04x}{}", "f".repeat(60)))
            .collect();
        for sha in &digests {
            setup
                .register_artifact(&project.project_id, "o", sha.clone(), "art", None)
                .unwrap();
        }

        std::thread::scope(|scope| {
            for sha in &digests {
                let root = root.clone();
                let project_id = project.project_id.clone();
                let claim_id = claim.claim_id.clone();
                let sha = sha.clone();
                scope.spawn(move || {
                    ProjectStore::new(&root)
                        .attach_evidence(&project_id, "o", &claim_id, sha, "l", None)
                        .unwrap();
                });
            }
        });

        let graph = setup.load_graph(&project.project_id).unwrap();
        assert_eq!(graph.edges.len(), digests.len(), "edges were lost");
        for sha in &digests {
            assert!(
                graph
                    .nodes
                    .contains_key(&ProjectStore::artifact_node_id(sha))
            );
        }
        assert_eq!(
            setup
                .load_claim(&project.project_id, &claim.claim_id)
                .unwrap()
                .status,
            ClaimStatus::EvidenceAttached
        );
    }

    #[test]
    fn interrupted_propose_claim_is_repaired_forward() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.create_project("o", "T", "Q").unwrap();
        let claim = store.propose_claim(&p.project_id, "o", "s", "sci").unwrap();

        // Crash point: claim record landed, graph write never did.
        clobber_graph(
            &store,
            &p.project_id,
            &EvidenceGraph::new(p.project_id.clone()),
        );

        let report = store.recover_project(&p.project_id).unwrap();
        assert!(report.repaired());
        assert_eq!(report.claim_nodes_restored, vec![claim.claim_id.clone()]);
        let graph = store.load_graph(&p.project_id).unwrap();
        let node = graph
            .nodes
            .get(&claim.evidence_node_id.clone().unwrap())
            .expect("claim node restored");
        assert_eq!(node.label, "s");
        assert_eq!(node.created_by, "sci");

        // Evidence can be attached again straight after recovery.
        let sha = "a".repeat(64);
        store
            .register_artifact(&p.project_id, "o", sha.clone(), "art", None)
            .unwrap();
        store
            .attach_evidence(&p.project_id, "o", &claim.claim_id, sha, "l", None)
            .unwrap();
    }

    #[test]
    fn interrupted_attach_evidence_is_repaired_forward() {
        let dir = tempdir().unwrap();
        let (store, p, claim, sha) = seeded(dir.path());
        store
            .attach_evidence(&p.project_id, "o", &claim.claim_id, sha, "l", None)
            .unwrap();

        // Crash point: the graph edge landed, the claim status write did not.
        let mut stale = store.load_claim(&p.project_id, &claim.claim_id).unwrap();
        stale.status = ClaimStatus::Proposed;
        store.write_claim_file(&stale).unwrap();

        let report = store.recover_project(&p.project_id).unwrap();
        assert_eq!(report.claims_advanced, vec![claim.claim_id.clone()]);
        assert_eq!(
            store
                .load_claim(&p.project_id, &claim.claim_id)
                .unwrap()
                .status,
            ClaimStatus::EvidenceAttached
        );
    }

    #[test]
    fn recovery_drops_graph_nodes_no_claim_record_explains() {
        let dir = tempdir().unwrap();
        let (store, p, claim, sha) = seeded(dir.path());
        store
            .attach_evidence(&p.project_id, "o", &claim.claim_id, sha.clone(), "l", None)
            .unwrap();

        // Crash point in the pre-fix write order: a graph claim node with no
        // claim record, plus an edge pointing at it.
        let mut graph = store.load_graph(&p.project_id).unwrap();
        let ghost = NodeId("node-claim-ghost".into());
        graph.nodes.insert(
            ghost.clone(),
            EvidenceNode {
                node_id: ghost.clone(),
                kind: NodeKind::Claim,
                project_id: p.project_id.clone(),
                label: "ghost".into(),
                artifact_sha256: None,
                run_id: None,
                created_by: "sci".into(),
                created_at: Utc::now(),
                metadata: BTreeMap::new(),
            },
        );
        graph.edges.push(EvidenceEdge {
            source: ProjectStore::artifact_node_id(&sha),
            target: ghost.clone(),
            relation: EdgeKind::Supports,
            actor: "o".into(),
            timestamp: Utc::now(),
            run_id: "none".into(),
            supporting_artifact_sha256: sha,
            confidence_kind: "high".into(),
        });
        clobber_graph(&store, &p.project_id, &graph);

        let report = store.recover_project(&p.project_id).unwrap();
        assert_eq!(report.orphan_nodes_removed, vec![ghost.0.clone()]);
        assert_eq!(report.orphan_edges_removed, 1);
        let graph = store.load_graph(&p.project_id).unwrap();
        assert!(!graph.nodes.contains_key(&ghost));
        assert!(graph.validate_integrity().is_ok());
        // The real claim and its evidence are untouched.
        assert_eq!(graph.edges.len(), 1);
    }

    #[test]
    fn recovery_sweeps_stale_temp_files_and_is_idempotent() {
        let dir = tempdir().unwrap();
        let (store, p, claim, sha) = seeded(dir.path());
        store
            .attach_evidence(&p.project_id, "o", &claim.claim_id, sha, "l", None)
            .unwrap();

        let project_dir = store.project_dir(&p.project_id);
        fs::write(project_dir.join(format!("{TEMP_PREFIX}dead.tmp")), b"{}").unwrap();
        fs::write(
            project_dir
                .join("claims")
                .join(format!("{TEMP_PREFIX}dead.tmp")),
            b"{}",
        )
        .unwrap();

        let report = store.recover_project(&p.project_id).unwrap();
        assert_eq!(report.stale_temp_files_removed, 2);
        assert!(temp_files_under(dir.path()).is_empty());

        // A healthy project needs no repair.
        let second = store.recover_project(&p.project_id).unwrap();
        assert!(!second.repaired(), "recovery is not idempotent: {second:?}");
        assert_eq!(second, ProjectRecoveryReport::default());
    }

    #[test]
    fn recover_project_rejects_unknown_project() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        assert!(store.recover_project(&ProjectId("nope".into())).is_err());
    }

    #[test]
    fn records_survive_a_failed_write_without_litter() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.create_project("o", "T", "Q").unwrap();
        let before = fs::read(store.project_dir(&p.project_id).join("graph.json")).unwrap();

        // A graph that fails validation must not touch the on-disk record.
        let mut bad = store.load_graph(&p.project_id).unwrap();
        bad.edges.push(EvidenceEdge {
            source: NodeId("ghost".into()),
            target: NodeId("other".into()),
            relation: EdgeKind::Supports,
            actor: "o".into(),
            timestamp: Utc::now(),
            run_id: "r".into(),
            supporting_artifact_sha256: "a".repeat(64),
            confidence_kind: "high".into(),
        });
        assert!(store.save_graph(&bad).is_err());
        assert_eq!(
            fs::read(store.project_dir(&p.project_id).join("graph.json")).unwrap(),
            before
        );
        assert!(temp_files_under(dir.path()).is_empty());
    }

    #[test]
    fn feature_gate_blocks_when_disabled() {
        let dir = tempdir().unwrap();
        let mut store = ProjectStore::new(dir.path());
        store.gates_mut().set(
            ScienceFeature::ResearchProject,
            crate::features::GateState::Disabled,
        );
        assert!(matches!(
            store.create_project("o", "t", "q"),
            Err(ScienceError::FeatureDisabled(_))
        ));
    }

    #[test]
    fn project_transition_lifecycle() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.create_project("o", "t", "q").unwrap();
        let p = store
            .transition_project(&p.project_id, "o", ProjectStatus::Planned)
            .unwrap();
        assert_eq!(p.status, ProjectStatus::Planned);
        let p = store
            .transition_project(&p.project_id, "o", ProjectStatus::Active)
            .unwrap();
        assert_eq!(p.status, ProjectStatus::Active);
    }
}
