//! V1 run → ResearchProject migration data contract. Seam contract: LS5-13.
//!
//! This module deliberately does not write a [`super::store::ProjectStore`].
//! It turns one already-succeeded [`crate::ScienceStore`] run into two typed
//! capabilities:
//!
//! - [`MigrationAdmission`] is captured before permission. It binds the exact
//!   run, artifact registry, evidence registry, provenance registry, request
//!   metadata, target project, and actor-owned authority run.
//! - [`VerifiedMigrationBundle`] is minted only after Finish reopens the source
//!   through the retained store capability, re-hashes every bounded payload,
//!   and proves that nothing changed while permission was pending.
//!
//! SessionActor remains responsible for approval, durable execution, commit
//! ordering, and recovery. ProjectStore consumes only a verified bundle; an ACP
//! adapter must never be able to turn a caller-supplied hash into success.

use super::evidence_graph::validate_sha256_hex;
use super::model::{OwnerId, ProjectId as ResearchProjectId, ResearchProject, validate_project_id};
use crate::{
    Artifact, CallId, Evidence, Provenance, Result, RunContext, RunId, RunRecord, RunState,
    ScienceError, ScienceStore,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const MIGRATION_ADMISSION_SCHEMA_VERSION: u32 = 1;
pub const MIGRATION_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const MIGRATION_COMMIT_SCHEMA_VERSION: u32 = 1;
pub const MAX_MIGRATION_ARTIFACTS: usize = 256;
pub const MAX_MIGRATION_RECORDS: usize = 1_024;
pub const MAX_MIGRATION_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_MIGRATION_METADATA_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_MIGRATION_TEXT_BYTES: usize = 64 * 1024;

/// Content-addressed snapshot captured before the operator is asked to Allow.
///
/// Fields stay private so callers may retain and serialize the capability, but
/// cannot rewrite one registry digest while leaving the others apparently
/// approved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationSnapshot {
    source_run_id: RunId,
    run_sha256: String,
    artifacts_sha256: String,
    evidence_sha256: String,
    provenance_sha256: String,
    payloads_sha256: String,
    artifact_count: usize,
    evidence_count: usize,
    provenance_count: usize,
    total_bytes: u64,
}

impl MigrationSnapshot {
    pub fn source_run_id(&self) -> &RunId {
        &self.source_run_id
    }

    pub fn artifact_count(&self) -> usize {
        self.artifact_count
    }

    pub fn evidence_count(&self) -> usize {
        self.evidence_count
    }

    pub fn provenance_count(&self) -> usize {
        self.provenance_count
    }

    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    pub fn sha256(&self) -> Result<String> {
        sha256_json(self)
    }
}

/// Immutable request and source-state capability admitted before permission.
///
/// `capture` is the only constructor. Finish must call [`Self::revalidate`]
/// with the actor's durable run context; merely deserializing this value never
/// produces a [`VerifiedMigrationBundle`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationAdmission {
    schema_version: u32,
    operation_id: String,
    source_run_id: RunId,
    source_project_id: crate::ProjectId,
    target_project_id: ResearchProjectId,
    authority_run_id: RunId,
    owner_id: String,
    session_id: String,
    workspace_root: PathBuf,
    artifact_root: PathBuf,
    title: String,
    research_question: String,
    source_snapshot: MigrationSnapshot,
}

impl MigrationAdmission {
    #[allow(clippy::too_many_arguments)]
    pub fn capture(
        store: &ScienceStore,
        authority_context: &RunContext,
        source_run_id: RunId,
        operation_id: impl Into<String>,
        target_project_id: ResearchProjectId,
        authority_run_id: RunId,
        title: impl Into<String>,
        research_question: impl Into<String>,
    ) -> Result<Self> {
        let operation_id = operation_id.into();
        let title = title.into();
        let research_question = research_question.into();
        validate_admission_request(
            store,
            authority_context,
            &source_run_id,
            &operation_id,
            &target_project_id,
            &authority_run_id,
            &title,
            &research_question,
        )?;
        let bundle = load_verified_source(store, authority_context, &source_run_id)?;
        if bundle.source_run.context.project_id.0 == target_project_id.0 {
            return Err(ScienceError::Invalid(
                "migration target project must differ from the source run project".into(),
            ));
        }
        let admission = Self {
            schema_version: MIGRATION_ADMISSION_SCHEMA_VERSION,
            operation_id,
            source_run_id,
            source_project_id: bundle.source_run.context.project_id.clone(),
            target_project_id,
            authority_run_id,
            owner_id: authority_context.owner_id.clone(),
            session_id: authority_context.session_id.clone(),
            workspace_root: authority_context.workspace_root.clone(),
            artifact_root: authority_context.artifact_root.clone(),
            title,
            research_question,
            source_snapshot: bundle.snapshot.clone(),
        };
        admission.validate(authority_context, store)?;
        Ok(admission)
    }

    pub fn sha256(&self) -> Result<String> {
        sha256_json(self)
    }

    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub fn source_run_id(&self) -> &RunId {
        &self.source_run_id
    }

    pub fn source_project_id(&self) -> &crate::ProjectId {
        &self.source_project_id
    }

    pub fn target_project_id(&self) -> &ResearchProjectId {
        &self.target_project_id
    }

    pub fn authority_run_id(&self) -> &RunId {
        &self.authority_run_id
    }

    pub fn owner_id(&self) -> &str {
        &self.owner_id
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub fn artifact_root(&self) -> &Path {
        &self.artifact_root
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn research_question(&self) -> &str {
        &self.research_question
    }

    pub fn source_snapshot(&self) -> &MigrationSnapshot {
        &self.source_snapshot
    }

    /// Mint the finish-only source capability after proving the exact
    /// authority run is durably Running with one matching Allow.
    ///
    /// This is the only public path to [`VerifiedMigrationBundle`]. Merely
    /// capturing or deserializing an admission cannot authorize
    /// `ProjectStore` writes.
    pub fn authorize_after_allow(
        &self,
        store: &ScienceStore,
        authority_context: &RunContext,
    ) -> Result<VerifiedMigrationBundle> {
        self.validate_authority_allow(store, authority_context)?;
        let mut bundle = self.revalidate_source(store, authority_context)?;
        bundle.authority_store = Some(store.clone());
        bundle.authority_context = Some(authority_context.clone());
        Ok(bundle)
    }

    fn validate_authority_allow(
        &self,
        store: &ScienceStore,
        authority_context: &RunContext,
    ) -> Result<()> {
        use crate::ApprovalDecision;

        self.validate(authority_context, store)?;
        let authority = store.load_run(&self.authority_run_id)?;
        if authority.state != RunState::Running || authority.context != *authority_context {
            return Err(ScienceError::Invalid(
                "migration Finish requires its exact Running authority run".into(),
            ));
        }
        if authority
            .context
            .environment
            .get("project_migration_admission_sha256")
            != Some(&self.sha256()?)
        {
            return Err(ScienceError::Invalid(
                "migration authority run is not bound to its admitted source digest".into(),
            ));
        }
        let approvals = store.approvals(&self.authority_run_id)?;
        let [approval] = approvals.as_slice() else {
            return Err(ScienceError::Invalid(
                "migration Finish requires exactly one durable authority approval".into(),
            ));
        };
        if approval.project_id != authority.context.project_id
            || approval.run_id != authority.context.run_id
            || approval.call_id != CallId::new("science_project_mutation")
            || approval.owner_id != authority.context.owner_id
            || approval.decision != ApprovalDecision::Allow
            || approval.decided_at.is_none()
        {
            return Err(ScienceError::Invalid(
                "migration Finish is not backed by the original durable Allow".into(),
            ));
        }
        Ok(())
    }

    /// Repeat every bounded metadata read and payload read after Allow.
    fn revalidate_source(
        &self,
        store: &ScienceStore,
        authority_context: &RunContext,
    ) -> Result<VerifiedMigrationBundle> {
        let mut bundle = load_verified_source(store, authority_context, &self.source_run_id)?;
        if bundle.source_run.context.project_id != self.source_project_id
            || bundle.snapshot != self.source_snapshot
        {
            return Err(ScienceError::Invalid(format!(
                "migration source run {} changed after admission",
                self.source_run_id.0
            )));
        }
        bundle.admission = Some(self.clone());
        Ok(bundle)
    }

    fn validate(&self, authority_context: &RunContext, store: &ScienceStore) -> Result<()> {
        if self.schema_version != MIGRATION_ADMISSION_SCHEMA_VERSION
            || self.authority_run_id != authority_context.run_id
            || self.target_project_id.0 != authority_context.project_id.0
            || self.owner_id != authority_context.owner_id
            || self.session_id != authority_context.session_id
            || self.workspace_root != authority_context.workspace_root
            || self.artifact_root != authority_context.artifact_root
        {
            return Err(ScienceError::Ownership);
        }
        validate_admission_request(
            store,
            authority_context,
            &self.source_run_id,
            &self.operation_id,
            &self.target_project_id,
            &self.authority_run_id,
            &self.title,
            &self.research_question,
        )?;
        if self.source_snapshot.source_run_id != self.source_run_id
            || self.source_snapshot.artifact_count == 0
            || self.source_snapshot.evidence_count == 0
            || self.source_snapshot.provenance_count == 0
        {
            return Err(ScienceError::Invalid(
                "migration admission has an invalid or empty source snapshot".into(),
            ));
        }
        Ok(())
    }
}

/// Artifact metadata safe to persist in the target project and migration
/// manifest. Payload bytes exist only inside [`VerifiedMigrationBundle`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigratedArtifact {
    pub source_run_id: RunId,
    pub source_call_id: CallId,
    pub source_relative_path: PathBuf,
    pub target_relative_path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
    pub mime: String,
    pub preview: String,
}

#[derive(Debug)]
struct VerifiedArtifactPayload {
    record: MigratedArtifact,
    payload: Vec<u8>,
}

/// Finish-only capability containing bounded, byte-verified source material.
///
/// The constructor and payload fields are private. Callers can inspect or copy
/// exact payloads through accessors, but cannot manufacture this type from
/// caller-supplied digests.
#[derive(Debug)]
pub struct VerifiedMigrationBundle {
    admission: Option<MigrationAdmission>,
    authority_store: Option<ScienceStore>,
    authority_context: Option<RunContext>,
    source_run: RunRecord,
    artifacts: Vec<VerifiedArtifactPayload>,
    evidence: Vec<Evidence>,
    provenance: Vec<Provenance>,
    snapshot: MigrationSnapshot,
}

impl VerifiedMigrationBundle {
    pub fn admission(&self) -> Result<&MigrationAdmission> {
        self.admission.as_ref().ok_or_else(|| {
            ScienceError::Invalid(
                "migration bundle was not minted by finish-time admission revalidation".into(),
            )
        })
    }

    pub fn source_run(&self) -> &RunRecord {
        &self.source_run
    }

    pub fn artifact_records(&self) -> impl ExactSizeIterator<Item = &MigratedArtifact> {
        self.artifacts.iter().map(|item| &item.record)
    }

    pub fn artifacts(&self) -> impl ExactSizeIterator<Item = (&MigratedArtifact, &[u8])> {
        self.artifacts
            .iter()
            .map(|item| (&item.record, item.payload.as_slice()))
    }

    pub fn evidence(&self) -> &[Evidence] {
        &self.evidence
    }

    pub fn provenance(&self) -> &[Provenance] {
        &self.provenance
    }

    pub fn snapshot(&self) -> &MigrationSnapshot {
        &self.snapshot
    }

    pub fn total_bytes(&self) -> u64 {
        self.snapshot.total_bytes
    }

    pub fn manifest(&self, generated_at: DateTime<Utc>) -> Result<MigrationManifest> {
        MigrationManifest::from_verified(self.admission()?, self, generated_at)
    }

    /// Reopen the authority and source immediately before any project-store
    /// admission/publication. This prevents a caller from minting a bundle
    /// under Allow, terminalizing or mutating its run, and using the stale
    /// capability later.
    pub(crate) fn verify_live_authority(&self) -> Result<()> {
        let admission = self.admission()?;
        let store = self.authority_store.as_ref().ok_or_else(|| {
            ScienceError::Invalid("migration bundle has no retained authority store".into())
        })?;
        let context = self.authority_context.as_ref().ok_or_else(|| {
            ScienceError::Invalid("migration bundle has no retained authority context".into())
        })?;
        admission.validate_authority_allow(store, context)?;
        let reopened = admission.revalidate_source(store, context)?;
        if reopened.snapshot != self.snapshot {
            return Err(ScienceError::Invalid(
                "migration source changed after the authority capability was minted".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn verify_live_authority_for_project_store(
        &self,
        project_store: &super::ProjectStore,
    ) -> Result<()> {
        self.verify_live_authority()?;
        let store = self.authority_store.as_ref().ok_or_else(|| {
            ScienceError::Invalid("migration bundle has no retained authority store".into())
        })?;
        if !store.shares_root_capability_with(project_store)? {
            return Err(ScienceError::Invalid(
                "migration ScienceStore and ProjectStore retained different roots".into(),
            ));
        }
        Ok(())
    }

    /// Keep the authority's write lock from final Running+Allow validation
    /// through the caller's ProjectStore journal/publication closure. The
    /// caller must already hold the ProjectStore guard so all cross-store
    /// operations preserve Project -> Science lock ordering.
    pub(crate) fn with_live_authority_for_project_store<T>(
        &self,
        project_store: &super::ProjectStore,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let store = self.authority_store.as_ref().ok_or_else(|| {
            ScienceError::Invalid("migration bundle has no retained authority store".into())
        })?;
        store.with_exclusive_authority(|| {
            self.verify_live_authority_for_project_store(project_store)?;
            operation()
        })
    }

    pub(crate) fn mark_project_commit_fence(&self) -> Result<()> {
        let admission = self.admission()?;
        let store = self.authority_store.as_ref().ok_or_else(|| {
            ScienceError::Invalid("migration bundle has no retained authority store".into())
        })?;
        let context = self.authority_context.as_ref().ok_or_else(|| {
            ScienceError::Invalid("migration bundle has no retained authority context".into())
        })?;
        if admission.authority_run_id() != &context.run_id
            || admission.target_project_id().0 != context.project_id.0
        {
            return Err(ScienceError::Ownership);
        }
        store.mark_authority_commit_fence_unlocked(
            context,
            &CallId::new("science_project_mutation"),
            admission.operation_id(),
        )
    }

    /// Before ProjectStore publishes a migration, reopen every target-owned
    /// copy through the retained Running+Allow authority capability. A source
    /// bundle alone cannot authorize project records that still point only at
    /// legacy source bytes.
    pub(crate) fn verify_target_copies_for_project_store(
        &self,
        project_store: &super::ProjectStore,
    ) -> Result<()> {
        use sha2::Digest as _;

        self.verify_live_authority_for_project_store(project_store)?;
        let admission = self.admission()?;
        let store = self.authority_store.as_ref().ok_or_else(|| {
            ScienceError::Invalid("migration bundle has no retained authority store".into())
        })?;
        let context = self.authority_context.as_ref().ok_or_else(|| {
            ScienceError::Invalid("migration bundle has no retained authority context".into())
        })?;
        let call_id = CallId::new("science_project_mutation");
        for item in &self.artifacts {
            let bytes = store.allowed_running_artifact_bytes(
                &context.project_id,
                &context.run_id,
                &context.owner_id,
                &call_id,
                &item.record.target_relative_path,
            )?;
            if bytes.as_slice() != item.payload.as_slice()
                || bytes.len() as u64 != item.record.bytes
                || format!("{:x}", Sha256::digest(&bytes)) != item.record.sha256
            {
                return Err(ScienceError::Invalid(
                    "migration target-owned copy differs from its verified source bundle".into(),
                ));
            }
        }
        if admission.authority_run_id() != &context.run_id {
            return Err(ScienceError::Ownership);
        }
        Ok(())
    }
}

/// Exact, human-auditable source material used to build the target project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationManifest {
    pub schema_version: u32,
    pub operation_id: String,
    pub admission_sha256: String,
    pub source_run: RunRecord,
    pub target_project_id: ResearchProjectId,
    pub authority_run_id: RunId,
    pub title: String,
    pub research_question: String,
    pub artifacts: Vec<MigratedArtifact>,
    pub evidence: Vec<Evidence>,
    pub provenance: Vec<Provenance>,
    pub source_snapshot: MigrationSnapshot,
    pub generated_at: DateTime<Utc>,
}

impl MigrationManifest {
    pub fn from_verified(
        admission: &MigrationAdmission,
        bundle: &VerifiedMigrationBundle,
        generated_at: DateTime<Utc>,
    ) -> Result<Self> {
        if bundle.admission()? != admission
            || bundle.source_run.context.project_id != admission.source_project_id
            || bundle.snapshot != admission.source_snapshot
        {
            return Err(ScienceError::Invalid(
                "verified migration bundle does not match its admission".into(),
            ));
        }
        Ok(Self {
            schema_version: MIGRATION_MANIFEST_SCHEMA_VERSION,
            operation_id: admission.operation_id.clone(),
            admission_sha256: admission.sha256()?,
            source_run: bundle.source_run.clone(),
            target_project_id: admission.target_project_id.clone(),
            authority_run_id: admission.authority_run_id.clone(),
            title: admission.title.clone(),
            research_question: admission.research_question.clone(),
            artifacts: bundle.artifact_records().cloned().collect(),
            evidence: bundle.evidence.clone(),
            provenance: bundle.provenance.clone(),
            source_snapshot: bundle.snapshot.clone(),
            generated_at,
        })
    }

    pub fn sha256(&self) -> Result<String> {
        sha256_json(self)
    }

    pub fn verify_against_admission(&self, admission: &MigrationAdmission) -> Result<()> {
        if self.schema_version != MIGRATION_MANIFEST_SCHEMA_VERSION
            || self.operation_id != admission.operation_id
            || self.admission_sha256 != admission.sha256()?
            || self.source_run.context.run_id != admission.source_run_id
            || self.source_run.context.project_id != admission.source_project_id
            || self.source_run.context.owner_id != admission.owner_id
            || self.source_run.context.session_id != admission.session_id
            || self.source_run.context.workspace_root != admission.workspace_root
            || self.source_run.context.artifact_root != admission.artifact_root
            || self.source_run.state != RunState::Succeeded
            || self.target_project_id != admission.target_project_id
            || self.authority_run_id != admission.authority_run_id
            || self.title != admission.title
            || self.research_question != admission.research_question
            || self.artifacts.is_empty()
            || self.evidence.is_empty()
            || self.provenance.is_empty()
        {
            return Err(ScienceError::Invalid(
                "migration manifest does not match its admission".into(),
            ));
        }
        let mut total_bytes = 0u64;
        let mut source_artifacts = Vec::with_capacity(self.artifacts.len());
        for migrated in &self.artifacts {
            migrated.source_run_id.validate()?;
            migrated.source_call_id.validate()?;
            validate_sha256_hex(&migrated.sha256).map_err(ScienceError::Invalid)?;
            if migrated.source_run_id != admission.source_run_id
                || migrated.target_relative_path
                    != migrated_target_path(&migrated.source_run_id, &migrated.source_relative_path)
            {
                return Err(ScienceError::Invalid(
                    "migration manifest artifact identity is invalid".into(),
                ));
            }
            total_bytes = total_bytes
                .checked_add(migrated.bytes)
                .ok_or_else(|| ScienceError::Invalid("migration byte count overflow".into()))?;
            if total_bytes > MAX_MIGRATION_BYTES {
                return Err(ScienceError::Invalid(format!(
                    "migration manifest exceeds the {MAX_MIGRATION_BYTES}-byte cap"
                )));
            }
            if !self
                .evidence
                .iter()
                .any(|item| item.artifact_sha256.as_deref() == Some(&migrated.sha256))
                || !self
                    .provenance
                    .iter()
                    .any(|item| item.input_sha256 == migrated.sha256)
            {
                return Err(ScienceError::Invalid(
                    "migration manifest artifact is not bound by evidence and provenance".into(),
                ));
            }
            source_artifacts.push(Artifact {
                run_id: migrated.source_run_id.clone(),
                call_id: migrated.source_call_id.clone(),
                relative_path: migrated.source_relative_path.clone(),
                sha256: migrated.sha256.clone(),
                bytes: migrated.bytes,
                mime: migrated.mime.clone(),
                preview: migrated.preview.clone(),
            });
        }
        let snapshot = migration_snapshot(
            &self.source_run,
            &source_artifacts,
            &self.evidence,
            &self.provenance,
            total_bytes,
        )?;
        if snapshot != self.source_snapshot || snapshot != admission.source_snapshot {
            return Err(ScienceError::Invalid(
                "migration manifest source snapshot mismatch".into(),
            ));
        }
        Ok(())
    }
}

/// Operation-addressed recovery journal content.
///
/// ProjectStore should publish this after Allow and source revalidation, before
/// the multi-record project bundle. A retry can therefore recover the same
/// deterministic target and authority run rather than minting a duplicate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationCommit {
    pub schema_version: u32,
    pub operation_id: String,
    pub request_sha256: String,
    pub admission: MigrationAdmission,
    pub manifest: MigrationManifest,
    pub manifest_sha256: String,
}

impl MigrationCommit {
    pub fn new(
        request_sha256: impl Into<String>,
        admission: MigrationAdmission,
        manifest: MigrationManifest,
    ) -> Result<Self> {
        let request_sha256 = request_sha256.into();
        validate_sha256_hex(&request_sha256).map_err(ScienceError::Invalid)?;
        manifest.verify_against_admission(&admission)?;
        let manifest_sha256 = manifest.sha256()?;
        Ok(Self {
            schema_version: MIGRATION_COMMIT_SCHEMA_VERSION,
            operation_id: admission.operation_id.clone(),
            request_sha256,
            admission,
            manifest,
            manifest_sha256,
        })
    }

    pub fn verify(&self) -> Result<()> {
        if self.schema_version != MIGRATION_COMMIT_SCHEMA_VERSION {
            return Err(ScienceError::Invalid(
                "unsupported migration commit schema".into(),
            ));
        }
        if self.operation_id != self.admission.operation_id
            || self.operation_id != self.manifest.operation_id
        {
            return Err(ScienceError::Invalid(
                "migration commit operation id does not match its admitted content".into(),
            ));
        }
        validate_sha256_hex(&self.request_sha256).map_err(ScienceError::Invalid)?;
        validate_sha256_hex(&self.manifest_sha256).map_err(ScienceError::Invalid)?;
        if self.manifest_sha256 != self.manifest.sha256()? {
            return Err(ScienceError::Invalid(
                "migration commit manifest digest mismatch".into(),
            ));
        }
        let rebuilt = Self::new(
            self.request_sha256.clone(),
            self.admission.clone(),
            self.manifest.clone(),
        )?;
        if rebuilt != *self {
            return Err(ScienceError::Invalid(
                "migration commit does not match its admitted content".into(),
            ));
        }
        Ok(())
    }
}

/// Proof that an already-Allowed migration authority run still owns the exact
/// target bytes named by a durable commit journal.
///
/// This capability supports crash recovery after the project bundle landed
/// but before the generic operation ledger or authority evidence finished.
/// Its fields are private and it can only be minted by reopening the retained
/// ScienceStore run, approval, and copied payloads.
#[derive(Debug)]
pub struct MigrationRecoveryGrant {
    operation_id: String,
    request_sha256: String,
    target_project_id: ResearchProjectId,
    authority_run_id: RunId,
    authority_state: RunState,
    authority_store: ScienceStore,
    commit: MigrationCommit,
}

impl MigrationRecoveryGrant {
    pub fn verify(store: &ScienceStore, commit: &MigrationCommit) -> Result<Self> {
        use crate::ApprovalDecision;

        commit.verify()?;
        let authority_run_id = commit.manifest.authority_run_id.clone();
        let run = store.load_run(&authority_run_id)?;
        if !matches!(run.state, RunState::Running | RunState::Succeeded)
            || run.context.run_id != authority_run_id
            || run.context.project_id.0 != commit.manifest.target_project_id.0
            || run.context.owner_id != commit.admission.owner_id
            || run.context.session_id != commit.admission.session_id
            || run.context.workspace_root != commit.admission.workspace_root
            || run.context.artifact_root != commit.admission.artifact_root
            || run.context.artifact_root != store.root().join("runs")
            || run
                .context
                .environment
                .get("project_migration_admission_sha256")
                != Some(&commit.admission.sha256()?)
        {
            return Err(ScienceError::Ownership);
        }
        let approvals = store.approvals(&authority_run_id)?;
        let [approval] = approvals.as_slice() else {
            return Err(ScienceError::Invalid(
                "migration recovery requires exactly one authority approval".into(),
            ));
        };
        let call_id = CallId::new("science_project_mutation");
        if approval.project_id != run.context.project_id
            || approval.run_id != authority_run_id
            || approval.owner_id != run.context.owner_id
            || approval.call_id != call_id
            || approval.decision != ApprovalDecision::Allow
            || approval.decided_at.is_none()
        {
            return Err(ScienceError::Invalid(
                "migration recovery is not backed by the original durable Allow".into(),
            ));
        }
        for artifact in &commit.manifest.artifacts {
            let bytes = match run.state {
                RunState::Running => store.allowed_running_artifact_bytes(
                    &run.context.project_id,
                    &authority_run_id,
                    &run.context.owner_id,
                    &call_id,
                    &artifact.target_relative_path,
                )?,
                RunState::Succeeded => store.artifact_bytes(
                    &run.context.project_id,
                    &authority_run_id,
                    &run.context.owner_id,
                    &artifact.target_relative_path,
                )?,
                _ => unreachable!("state checked above"),
            };
            if bytes.len() as u64 != artifact.bytes
                || format!("{:x}", Sha256::digest(&bytes)) != artifact.sha256
            {
                return Err(ScienceError::Invalid(
                    "migration recovery target artifact failed byte verification".into(),
                ));
            }
        }
        Ok(Self {
            operation_id: commit.operation_id.clone(),
            request_sha256: commit.request_sha256.clone(),
            target_project_id: commit.manifest.target_project_id.clone(),
            authority_run_id,
            authority_state: run.state,
            authority_store: store.clone(),
            commit: commit.clone(),
        })
    }

    fn revalidate_for_project_store(
        &self,
        project_store: &super::ProjectStore,
    ) -> Result<RunState> {
        let current = Self::verify(&self.authority_store, &self.commit)?;
        if !self
            .authority_store
            .shares_root_capability_with(project_store)?
            || current.operation_id != self.operation_id
            || current.request_sha256 != self.request_sha256
            || current.target_project_id != self.target_project_id
            || current.authority_run_id != self.authority_run_id
        {
            return Err(ScienceError::Invalid(
                "migration recovery authority changed after its grant was minted".into(),
            ));
        }
        Ok(current.authority_state)
    }

    /// Revalidate the retained authority and keep its write lock held while
    /// the caller verifies or publishes the project-side operation. The
    /// callback receives the current state so only a Running authority can
    /// publish missing records while Succeeded remains replay-only.
    pub(crate) fn with_revalidated_authority_for_project_store<T>(
        &self,
        project_store: &super::ProjectStore,
        operation: impl FnOnce(RunState) -> Result<T>,
    ) -> Result<T> {
        self.authority_store.with_exclusive_authority(|| {
            let current_state = self.revalidate_for_project_store(project_store)?;
            operation(current_state)
        })
    }

    pub(crate) fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub(crate) fn request_sha256(&self) -> &str {
        &self.request_sha256
    }

    pub(crate) fn verify_retained_commit(&self, current: &MigrationCommit) -> Result<()> {
        current.verify()?;
        if current != &self.commit {
            return Err(ScienceError::Invalid(
                "migration recovery journal differs from the grant's retained commit".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn mark_project_commit_fence(&self) -> Result<()> {
        let run = self.authority_store.load_run(&self.authority_run_id)?;
        if run.state != RunState::Running
            || run.context.run_id != self.authority_run_id
            || run.context.project_id.0 != self.target_project_id.0
        {
            return Err(ScienceError::Ownership);
        }
        self.authority_store.mark_authority_commit_fence_unlocked(
            &run.context,
            &CallId::new("science_project_mutation"),
            &self.operation_id,
        )
    }

    pub(crate) fn target_project_id(&self) -> &ResearchProjectId {
        &self.target_project_id
    }

    pub fn authority_run_id(&self) -> &RunId {
        &self.authority_run_id
    }

    pub fn authority_state(&self) -> RunState {
        self.authority_state
    }
}

/// Hash proof emitted only for a verified, non-empty artifact set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HashVerification {
    /// Emitted only by [`MigrationResult::from_commit`] after a non-empty,
    /// byte-verified bundle and its admission-bound manifest validate.
    Verified,
    Mismatch {
        expected: String,
        actual: String,
    },
    /// Compatibility result for the former hash-vs-hash helper. Equal strings
    /// are not evidence that any bytes were opened, so this is never success.
    UnverifiedLegacyComparison {
        digest: String,
    },
}

/// Result of one durable migration commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationResult {
    pub source_run_id: String,
    pub target_project_id: ResearchProjectId,
    pub authority_run_id: String,
    pub artifacts_migrated: usize,
    pub evidence_items_migrated: usize,
    pub provenance_items_migrated: usize,
    pub bytes_migrated: u64,
    pub admission_sha256: String,
    pub manifest_sha256: String,
    pub hash_verification: HashVerification,
}

impl MigrationResult {
    pub fn from_commit(commit: &MigrationCommit) -> Result<Self> {
        commit.verify()?;
        let snapshot = &commit.manifest.source_snapshot;
        if snapshot.artifact_count == 0 {
            return Err(ScienceError::Invalid(
                "a migration result cannot verify an empty artifact set".into(),
            ));
        }
        Ok(Self {
            source_run_id: commit.admission.source_run_id.0.clone(),
            target_project_id: commit.admission.target_project_id.clone(),
            authority_run_id: commit.admission.authority_run_id.0.clone(),
            artifacts_migrated: snapshot.artifact_count,
            evidence_items_migrated: snapshot.evidence_count,
            provenance_items_migrated: snapshot.provenance_count,
            bytes_migrated: snapshot.total_bytes,
            admission_sha256: commit.admission.sha256()?,
            manifest_sha256: commit.manifest_sha256.clone(),
            hash_verification: HashVerification::Verified,
        })
    }
}

/// Compatibility helpers that do not own approval or persistence.
pub struct V1ToV2Migration;

impl V1ToV2Migration {
    pub fn create_project_from_run(
        project_id: ResearchProjectId,
        owner_id: OwnerId,
        run_title: impl Into<String>,
        research_question: impl Into<String>,
        run_ids: Vec<String>,
    ) -> ResearchProject {
        let mut project = ResearchProject::new(
            project_id,
            owner_id,
            run_title.into(),
            research_question.into(),
        );
        for run_id in run_ids {
            project.add_session(run_id);
        }
        project
    }

    /// Legacy compatibility only. Comparing two caller-provided strings can
    /// detect a mismatch, but equal strings never become `Verified`.
    #[deprecated(note = "use MigrationAdmission::revalidate over actual artifact bytes")]
    pub fn verify_artifact_hash(original_sha256: &str, migrated_sha256: &str) -> HashVerification {
        if original_sha256 != migrated_sha256 {
            HashVerification::Mismatch {
                expected: original_sha256.to_string(),
                actual: migrated_sha256.to_string(),
            }
        } else {
            HashVerification::UnverifiedLegacyComparison {
                digest: original_sha256.to_string(),
            }
        }
    }

    pub fn is_successful(result: &MigrationResult) -> bool {
        matches!(result.hash_verification, HashVerification::Verified)
            && result.artifacts_migrated > 0
    }
}

fn validate_admission_request(
    store: &ScienceStore,
    authority_context: &RunContext,
    source_run_id: &RunId,
    operation_id: &str,
    target_project_id: &ResearchProjectId,
    authority_run_id: &RunId,
    title: &str,
    research_question: &str,
) -> Result<()> {
    source_run_id.validate()?;
    authority_run_id.validate()?;
    super::mutation::validate_operation_id(operation_id)?;
    validate_project_id(&target_project_id.0)?;
    validate_text("migration title", title)?;
    validate_text("migration research question", research_question)?;
    if source_run_id == authority_run_id
        || authority_run_id != &authority_context.run_id
        || target_project_id.0 != authority_context.project_id.0
        || authority_context.owner_id.is_empty()
        || authority_context.session_id.is_empty()
    {
        return Err(ScienceError::Invalid(
            "migration source, target, or authority-run identity is invalid".into(),
        ));
    }
    if authority_context.artifact_root != store.root().join("runs")
        || !store.root().starts_with(&authority_context.workspace_root)
    {
        return Err(ScienceError::Invalid(
            "migration store is outside the actor workspace or artifact root".into(),
        ));
    }
    Ok(())
}

fn validate_text(kind: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > MAX_MIGRATION_TEXT_BYTES || value.contains('\0') {
        return Err(ScienceError::Invalid(format!(
            "{kind} must be non-empty, contain no NUL, and be at most {MAX_MIGRATION_TEXT_BYTES} bytes"
        )));
    }
    Ok(())
}

fn load_verified_source(
    store: &ScienceStore,
    authority_context: &RunContext,
    source_run_id: &RunId,
) -> Result<VerifiedMigrationBundle> {
    let mut metadata_bytes = 0usize;
    let source_run =
        store.load_run_bounded(source_run_id, remaining_metadata_bytes(metadata_bytes)?)?;
    charge_metadata(&mut metadata_bytes, &source_run)?;
    if source_run.state != RunState::Succeeded {
        return Err(ScienceError::Invalid(
            "migration source run must be succeeded".into(),
        ));
    }
    if source_run.context.owner_id != authority_context.owner_id
        || source_run.context.session_id != authority_context.session_id
        || source_run.context.workspace_root != authority_context.workspace_root
        || source_run.context.artifact_root != authority_context.artifact_root
    {
        return Err(ScienceError::Ownership);
    }
    if source_run.context.artifact_root != store.root().join("runs") {
        return Err(ScienceError::Invalid(
            "migration source run is outside the retained science store".into(),
        ));
    }

    let artifacts = store.artifacts_bounded(
        source_run_id,
        MAX_MIGRATION_ARTIFACTS,
        remaining_metadata_bytes(metadata_bytes)?,
    )?;
    charge_metadata(&mut metadata_bytes, &artifacts)?;
    let evidence = store.evidence_bounded(
        source_run_id,
        MAX_MIGRATION_RECORDS,
        remaining_metadata_bytes(metadata_bytes)?,
    )?;
    charge_metadata(&mut metadata_bytes, &evidence)?;
    let provenance = store.provenance_bounded(
        source_run_id,
        MAX_MIGRATION_RECORDS,
        remaining_metadata_bytes(metadata_bytes)?,
    )?;
    charge_metadata(&mut metadata_bytes, &provenance)?;
    if artifacts.is_empty() || evidence.is_empty() || provenance.is_empty() {
        return Err(ScienceError::Invalid(
            "migration source requires non-empty artifacts, evidence, and provenance".into(),
        ));
    }

    for item in &evidence {
        if let Some(sha256) = &item.artifact_sha256 {
            validate_sha256_hex(sha256).map_err(ScienceError::Invalid)?;
            if !artifacts.iter().any(|artifact| artifact.sha256 == *sha256) {
                return Err(ScienceError::Invalid(
                    "migration evidence cites an artifact outside the source registry".into(),
                ));
            }
        }
    }

    let mut total_bytes = 0u64;
    let mut verified_artifacts = Vec::with_capacity(artifacts.len());
    for artifact in &artifacts {
        validate_sha256_hex(&artifact.sha256).map_err(ScienceError::Invalid)?;
        if !evidence
            .iter()
            .any(|item| item.artifact_sha256.as_deref() == Some(&artifact.sha256))
        {
            return Err(ScienceError::Invalid(format!(
                "migration artifact {} has no matching evidence",
                artifact.relative_path.display()
            )));
        }
        if !provenance
            .iter()
            .any(|item| item.input_sha256 == artifact.sha256)
        {
            return Err(ScienceError::Invalid(format!(
                "migration artifact {} has no matching provenance",
                artifact.relative_path.display()
            )));
        }
        let remaining = MAX_MIGRATION_BYTES.saturating_sub(total_bytes);
        if artifact.bytes > remaining {
            return Err(ScienceError::Invalid(format!(
                "migration source exceeds the {MAX_MIGRATION_BYTES}-byte cap"
            )));
        }
        let payload = store.artifact_bytes_bounded(
            &source_run.context.project_id,
            source_run_id,
            &authority_context.owner_id,
            &artifact.relative_path,
            remaining,
        )?;
        total_bytes = total_bytes
            .checked_add(payload.len() as u64)
            .ok_or_else(|| ScienceError::Invalid("migration byte count overflow".into()))?;
        if total_bytes > MAX_MIGRATION_BYTES {
            return Err(ScienceError::Invalid(format!(
                "migration source exceeds the {MAX_MIGRATION_BYTES}-byte cap"
            )));
        }
        verified_artifacts.push(VerifiedArtifactPayload {
            record: migrated_artifact(source_run_id, artifact),
            payload,
        });
    }

    let snapshot =
        migration_snapshot(&source_run, &artifacts, &evidence, &provenance, total_bytes)?;
    Ok(VerifiedMigrationBundle {
        admission: None,
        authority_store: None,
        authority_context: None,
        source_run,
        artifacts: verified_artifacts,
        evidence,
        provenance,
        snapshot,
    })
}

fn migrated_artifact(source_run_id: &RunId, artifact: &Artifact) -> MigratedArtifact {
    MigratedArtifact {
        source_run_id: source_run_id.clone(),
        source_call_id: artifact.call_id.clone(),
        source_relative_path: artifact.relative_path.clone(),
        target_relative_path: migrated_target_path(source_run_id, &artifact.relative_path),
        sha256: artifact.sha256.clone(),
        bytes: artifact.bytes,
        mime: artifact.mime.clone(),
        preview: artifact.preview.clone(),
    }
}

fn migrated_target_path(source_run_id: &RunId, source_relative_path: &Path) -> PathBuf {
    PathBuf::from("migrated")
        .join(&source_run_id.0)
        .join(source_relative_path)
}

fn migration_snapshot(
    run: &RunRecord,
    artifacts: &[Artifact],
    evidence: &[Evidence],
    provenance: &[Provenance],
    total_bytes: u64,
) -> Result<MigrationSnapshot> {
    let payload_records = artifacts
        .iter()
        .map(|artifact| {
            (
                artifact.relative_path.clone(),
                artifact.sha256.clone(),
                artifact.bytes,
            )
        })
        .collect::<Vec<_>>();
    Ok(MigrationSnapshot {
        source_run_id: run.context.run_id.clone(),
        run_sha256: sha256_json(run)?,
        artifacts_sha256: sha256_json(artifacts)?,
        evidence_sha256: sha256_json(evidence)?,
        provenance_sha256: sha256_json(provenance)?,
        payloads_sha256: sha256_json(&payload_records)?,
        artifact_count: artifacts.len(),
        evidence_count: evidence.len(),
        provenance_count: provenance.len(),
        total_bytes,
    })
}

fn sha256_json<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

fn charge_metadata<T: Serialize + ?Sized>(used: &mut usize, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value)?.len();
    *used = used
        .checked_add(bytes)
        .ok_or_else(|| ScienceError::Invalid("migration metadata size overflow".into()))?;
    if *used > MAX_MIGRATION_METADATA_BYTES {
        return Err(ScienceError::Invalid(format!(
            "migration metadata exceeds the {MAX_MIGRATION_METADATA_BYTES}-byte cap"
        )));
    }
    Ok(())
}

fn remaining_metadata_bytes(used: usize) -> Result<u64> {
    let remaining = MAX_MIGRATION_METADATA_BYTES
        .checked_sub(used)
        .ok_or_else(|| ScienceError::Invalid("migration metadata limit exhausted".into()))?;
    u64::try_from(remaining)
        .map_err(|_| ScienceError::Invalid("migration metadata limit does not fit u64".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Approval, ApprovalDecision, CallId, ProjectId, RunState};
    use std::collections::BTreeMap;
    use std::sync::{Arc, Barrier, mpsc};
    use std::time::Duration;
    use tempfile::TempDir;

    struct Fixture {
        _workspace: TempDir,
        store: ScienceStore,
        root: PathBuf,
        source_run_id: RunId,
        authority_context: RunContext,
    }

    fn fixture() -> Fixture {
        let workspace = tempfile::tempdir().unwrap();
        let canonical_workspace = dunce::canonicalize(workspace.path()).unwrap();
        let root = canonical_workspace.join("science-store");
        std::fs::create_dir_all(&root).unwrap();
        let store = ScienceStore::new_confined(&root, &canonical_workspace).unwrap();
        let source_run_id = RunId::new("source-run-1");
        let source_project_id = ProjectId::new("source-project-1");
        let source_context = RunContext {
            run_id: source_run_id.clone(),
            project_id: source_project_id.clone(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            workspace_root: canonical_workspace.clone(),
            provider: "offline".into(),
            approval_policy: "test".into(),
            tool_profile: "migration-source-test".into(),
            artifact_root: root.join("runs"),
            environment: BTreeMap::new(),
        };
        store.create_run(source_context).unwrap();
        let source_call = CallId::new("source-call");
        store
            .request_approval(Approval {
                project_id: source_project_id.clone(),
                run_id: source_run_id.clone(),
                call_id: source_call.clone(),
                owner_id: "owner-1".into(),
                decision: ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        store
            .transition(&source_run_id, RunState::AwaitingApproval, None)
            .unwrap();
        store
            .decide_approval(
                &source_project_id,
                &source_run_id,
                "owner-1",
                &source_call,
                ApprovalDecision::Allow,
            )
            .unwrap();
        store
            .transition(&source_run_id, RunState::Running, None)
            .unwrap();
        for (index, bytes) in [b"alpha".as_slice(), b"beta".as_slice()]
            .into_iter()
            .enumerate()
        {
            let relative = PathBuf::from(format!("artifact-{index}.bin"));
            let artifact = store
                .put_artifact(
                    &source_project_id,
                    &source_run_id,
                    "owner-1",
                    source_call.clone(),
                    &relative,
                    bytes,
                    "application/octet-stream",
                    format!("artifact {index}"),
                )
                .unwrap();
            store
                .add_evidence(Evidence {
                    run_id: source_run_id.clone(),
                    claim: format!("artifact {index} is preserved"),
                    source: format!("fixture://artifact/{index}"),
                    artifact_sha256: Some(artifact.sha256.clone()),
                    verified_at: Utc::now(),
                })
                .unwrap();
            store
                .add_provenance(Provenance {
                    run_id: source_run_id.clone(),
                    source_uri: format!("fixture://artifact/{index}"),
                    source_commit: None,
                    source_path: Some(relative.display().to_string()),
                    license: "CC0-1.0".into(),
                    retrieved_at: Utc::now(),
                    input_sha256: artifact.sha256,
                    tool: "migration-test".into(),
                    environment: BTreeMap::new(),
                })
                .unwrap();
        }
        store.transition_succeeded_verified(&source_run_id).unwrap();
        let authority_context = RunContext {
            run_id: RunId::new("authority-run-1"),
            project_id: ProjectId::new("target-project-1"),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            workspace_root: canonical_workspace,
            provider: "offline".into(),
            approval_policy: "production-session-permission".into(),
            tool_profile: "science-project-migration-v1".into(),
            artifact_root: root.join("runs"),
            environment: BTreeMap::new(),
        };
        Fixture {
            _workspace: workspace,
            store,
            root,
            source_run_id,
            authority_context,
        }
    }

    fn admission(fixture: &Fixture) -> MigrationAdmission {
        MigrationAdmission::capture(
            &fixture.store,
            &fixture.authority_context,
            fixture.source_run_id.clone(),
            "op-migrate-0001",
            ResearchProjectId("target-project-1".into()),
            fixture.authority_context.run_id.clone(),
            "Migrated study",
            "Which evidence remains valid?",
        )
        .unwrap()
    }

    fn begin_authority(fixture: &Fixture, admission: &MigrationAdmission) -> RunContext {
        let mut context = fixture.authority_context.clone();
        context.environment.insert(
            "project_migration_admission_sha256".into(),
            admission.sha256().unwrap(),
        );
        fixture.store.create_run(context.clone()).unwrap();
        fixture
            .store
            .request_approval(Approval {
                project_id: context.project_id.clone(),
                run_id: context.run_id.clone(),
                call_id: CallId::new("science_project_mutation"),
                owner_id: context.owner_id.clone(),
                decision: ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        fixture
            .store
            .transition(&context.run_id, RunState::AwaitingApproval, None)
            .unwrap();
        context
    }

    fn authorize(fixture: &Fixture, admission: &MigrationAdmission) -> VerifiedMigrationBundle {
        let context = begin_authority(fixture, admission);
        allow_authority(fixture, &context);
        admission
            .authorize_after_allow(&fixture.store, &context)
            .unwrap()
    }

    fn allow_authority(fixture: &Fixture, context: &RunContext) {
        fixture
            .store
            .decide_approval(
                &context.project_id,
                &context.run_id,
                &context.owner_id,
                &CallId::new("science_project_mutation"),
                ApprovalDecision::Allow,
            )
            .unwrap();
        fixture
            .store
            .transition(&context.run_id, RunState::Running, None)
            .unwrap();
    }

    #[test]
    fn capture_and_finish_revalidation_bind_real_bytes_and_all_registries() {
        let fixture = fixture();
        let admission = admission(&fixture);
        let bundle = authorize(&fixture, &admission);
        assert_eq!(bundle.artifact_records().len(), 2);
        assert_eq!(bundle.evidence().len(), 2);
        assert_eq!(bundle.provenance().len(), 2);
        assert_eq!(bundle.total_bytes(), 9);
        for (artifact, payload) in bundle.artifacts() {
            assert_eq!(artifact.bytes, payload.len() as u64);
            assert_eq!(artifact.sha256, crate::hex_sha256(payload));
            assert!(artifact.target_relative_path.starts_with("migrated"));
        }

        let manifest = MigrationManifest::from_verified(&admission, &bundle, Utc::now()).unwrap();
        let request_sha256 = "a".repeat(64);
        let commit = MigrationCommit::new(request_sha256, admission, manifest).unwrap();
        commit.verify().unwrap();
        let result = MigrationResult::from_commit(&commit).unwrap();
        assert!(V1ToV2Migration::is_successful(&result));
        assert_eq!(result.artifacts_migrated, 2);
        assert_eq!(result.evidence_items_migrated, 2);
        assert_eq!(result.provenance_items_migrated, 2);
        assert_eq!(result.bytes_migrated, 9);
    }

    #[test]
    fn migration_manifest_rejects_bundle_from_another_admission() {
        let fixture = fixture();
        let admitted = admission(&fixture);
        let bundle = authorize(&fixture, &admitted);
        let substituted = MigrationAdmission::capture(
            &fixture.store,
            &fixture.authority_context,
            fixture.source_run_id.clone(),
            "op-migrate-substituted",
            ResearchProjectId("target-project-1".into()),
            fixture.authority_context.run_id.clone(),
            "Migrated study",
            "Which evidence remains valid?",
        )
        .unwrap();
        assert_ne!(admitted, substituted);
        assert!(
            MigrationManifest::from_verified(&substituted, &bundle, Utc::now()).is_err(),
            "a verified bundle was relabeled with another admission"
        );
    }

    #[test]
    fn migration_authority_closure_blocks_terminal_transition_until_commit_returns() {
        let fixture = fixture();
        let admitted = admission(&fixture);
        let bundle = authorize(&fixture, &admitted);
        let project_store = super::super::ProjectStore::new_confined(
            &fixture.root,
            &fixture.authority_context.workspace_root,
        )
        .unwrap();
        let entered_commit = Arc::new(Barrier::new(2));
        let release_commit = Arc::new(Barrier::new(2));
        let (transition_started_tx, transition_started_rx) = mpsc::channel();
        let (transition_done_tx, transition_done_rx) = mpsc::channel();

        std::thread::scope(|scope| {
            let entered_commit_worker = entered_commit.clone();
            let release_commit_worker = release_commit.clone();
            scope.spawn(move || {
                let _project_guard = project_store.write_guard().unwrap();
                bundle
                    .with_live_authority_for_project_store(&project_store, || {
                        entered_commit_worker.wait();
                        release_commit_worker.wait();
                        Ok(())
                    })
                    .unwrap();
            });

            entered_commit.wait();
            scope.spawn(move || {
                transition_started_tx.send(()).unwrap();
                transition_done_tx
                    .send(fixture.store.transition(
                        &fixture.authority_context.run_id,
                        RunState::Failed,
                        Some("concurrent terminalization probe".into()),
                    ))
                    .unwrap();
            });
            transition_started_rx
                .recv_timeout(Duration::from_secs(2))
                .unwrap();
            assert!(
                transition_done_rx
                    .recv_timeout(Duration::from_millis(100))
                    .is_err(),
                "authority terminalized while the migration commit closure held its lease"
            );
            release_commit.wait();
            assert_eq!(
                transition_done_rx
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap()
                    .unwrap()
                    .state,
                RunState::Failed
            );
        });
    }

    #[test]
    fn finish_revalidation_detects_payload_tampering() {
        let fixture = fixture();
        let admission = admission(&fixture);
        std::fs::write(
            fixture
                .root
                .join("runs/source-run-1/artifacts/artifact-0.bin"),
            b"tampered",
        )
        .unwrap();
        let context = begin_authority(&fixture, &admission);
        allow_authority(&fixture, &context);
        let error = admission
            .authorize_after_allow(&fixture.store, &context)
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("artifact bytes do not match their registered hash/length")
        );
    }

    #[test]
    fn finish_revalidation_detects_every_registry_tamper() {
        for registry in ["artifacts.json", "evidence.json", "provenance.json"] {
            let fixture = fixture();
            let admission = admission(&fixture);
            let path = fixture.root.join("runs/source-run-1").join(registry);
            let mut value: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            value.as_array_mut().unwrap().reverse();
            std::fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
            let context = begin_authority(&fixture, &admission);
            allow_authority(&fixture, &context);
            let error = admission
                .authorize_after_allow(&fixture.store, &context)
                .unwrap_err();
            assert!(
                error.to_string().contains("changed after admission"),
                "{registry}: {error}"
            );
        }
    }

    #[test]
    fn capture_rejects_non_succeeded_and_cross_boundary_sources() {
        let fixture = fixture();
        let mut wrong_owner = fixture.authority_context.clone();
        wrong_owner.owner_id = "owner-2".into();
        assert!(
            MigrationAdmission::capture(
                &fixture.store,
                &wrong_owner,
                fixture.source_run_id.clone(),
                "op-migrate-owner",
                ResearchProjectId("target-project-1".into()),
                wrong_owner.run_id.clone(),
                "Title",
                "Question?",
            )
            .is_err()
        );

        let mut wrong_session = fixture.authority_context.clone();
        wrong_session.session_id = "session-2".into();
        assert!(
            MigrationAdmission::capture(
                &fixture.store,
                &wrong_session,
                fixture.source_run_id.clone(),
                "op-migrate-session",
                ResearchProjectId("target-project-1".into()),
                wrong_session.run_id.clone(),
                "Title",
                "Question?",
            )
            .is_err()
        );

        assert!(
            MigrationAdmission::capture(
                &fixture.store,
                &fixture.authority_context,
                RunId::new("missing-run"),
                "op-migrate-missing",
                ResearchProjectId("target-project-1".into()),
                fixture.authority_context.run_id.clone(),
                "Title",
                "Question?",
            )
            .is_err()
        );

        let pending = RunId::new("pending-source");
        let mut context = fixture.authority_context.clone();
        context.run_id = pending.clone();
        context.project_id = ProjectId::new("pending-project");
        fixture.store.create_run(context).unwrap();
        assert!(
            MigrationAdmission::capture(
                &fixture.store,
                &fixture.authority_context,
                pending,
                "op-migrate-pending",
                ResearchProjectId("target-project-1".into()),
                fixture.authority_context.run_id.clone(),
                "Title",
                "Question?",
            )
            .is_err()
        );
    }

    #[test]
    fn capture_requires_each_artifact_to_have_evidence_and_provenance() {
        for (registry, operation_id) in [
            ("evidence.json", "op-missing-evidence"),
            ("provenance.json", "op-missing-provenance"),
        ] {
            let fixture = fixture();
            let path = fixture.root.join("runs/source-run-1").join(registry);
            let mut value: Vec<serde_json::Value> =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            value.pop();
            std::fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
            let error = MigrationAdmission::capture(
                &fixture.store,
                &fixture.authority_context,
                fixture.source_run_id.clone(),
                operation_id,
                ResearchProjectId("target-project-1".into()),
                fixture.authority_context.run_id.clone(),
                "Title",
                "Question?",
            )
            .unwrap_err();
            assert!(
                error.to_string().contains("has no matching"),
                "{registry}: {error}"
            );
        }
    }

    #[test]
    #[allow(deprecated)]
    fn equal_caller_supplied_hash_strings_are_never_verified() {
        let result = V1ToV2Migration::verify_artifact_hash(&"a".repeat(64), &"a".repeat(64));
        assert!(matches!(
            result,
            HashVerification::UnverifiedLegacyComparison { .. }
        ));
    }

    #[test]
    fn commit_detects_manifest_tampering() {
        let fixture = fixture();
        let admission = admission(&fixture);
        let bundle = authorize(&fixture, &admission);
        let manifest = MigrationManifest::from_verified(&admission, &bundle, Utc::now()).unwrap();
        let mut commit = MigrationCommit::new("a".repeat(64), admission, manifest).unwrap();
        commit.manifest.title = "Changed after commit".into();
        assert!(commit.verify().is_err());
    }

    #[test]
    fn bundle_cannot_be_minted_before_durable_allow() {
        for (decision, terminal_state) in [
            (ApprovalDecision::Deny, RunState::Denied),
            (ApprovalDecision::Timeout, RunState::TimedOut),
            (ApprovalDecision::Cancel, RunState::Cancelled),
        ] {
            let fixture = fixture();
            let admission = admission(&fixture);
            let context = begin_authority(&fixture, &admission);
            assert!(
                admission
                    .authorize_after_allow(&fixture.store, &context)
                    .is_err(),
                "AwaitingApproval minted a bundle"
            );
            fixture
                .store
                .decide_approval(
                    &context.project_id,
                    &context.run_id,
                    &context.owner_id,
                    &CallId::new("science_project_mutation"),
                    decision.clone(),
                )
                .unwrap();
            fixture
                .store
                .transition(&context.run_id, terminal_state, None)
                .unwrap();
            assert!(
                admission
                    .authorize_after_allow(&fixture.store, &context)
                    .is_err(),
                "{decision:?} minted a bundle"
            );
            assert!(fixture.store.artifacts(&context.run_id).unwrap().is_empty());
        }
    }
}
