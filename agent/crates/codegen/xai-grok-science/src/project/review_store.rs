//! Durable, artifact-bound project review records.
//!
//! A review is not a desktop projection and it is not an assertion that a
//! path was inspected. The record is written only by the SessionActor's typed
//! project-mutation path, after `ProjectStore` reopens the cited succeeded run
//! from the same store and hashes the registered artifact bytes again.

use super::{model::ProjectId, store::ProjectStore};
use crate::{
    RunId, RunState, ScienceError, ScienceStore, features::ScienceFeature,
    project::evidence_graph::validate_sha256_hex,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Pass,
    Warn,
    Fail,
    NeedsRevision,
    Inconclusive,
}

impl ReviewVerdict {
    pub fn parse(value: &str) -> crate::Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "pass" => Ok(Self::Pass),
            "warn" => Ok(Self::Warn),
            "fail" => Ok(Self::Fail),
            "needs_revision" | "needsrevision" => Ok(Self::NeedsRevision),
            "inconclusive" => Ok(Self::Inconclusive),
            _ => Err(ScienceError::Invalid(
                "review verdict must be pass, warn, fail, needs_revision, or inconclusive".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewedArtifact {
    pub source_run_id: String,
    pub relative_path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
    pub mime: String,
}

/// Immutable proof of the exact review request and source authority inspected
/// before the SessionActor asks for permission.
///
/// Fields are deliberately private and the type is not deserializable.  The
/// only constructor reopens the source run and selected artifact bytes through
/// the same retained `ScienceStore` / `ProjectStore` root capability, while
/// holding the project write lock long enough to bind the current project
/// revision.  `apply_actor_review` captures the same proof again after Allow
/// and requires byte-for-byte equality before it may write a review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewAdmission {
    request_sha256: String,
    owner_id: String,
    project_id: ProjectId,
    session_id: String,
    source_run_id: String,
    authority_run_id: String,
    source_authority_sha256: String,
    artifacts: Vec<ReviewedArtifact>,
    project_revision: String,
    sha256: String,
}

/// Opaque authority for repairing only the narrow review-ledger/operation-ledger
/// crash window.
///
/// The grant retains the exact `ScienceStore` directory capability used to
/// verify the orphan. It is intentionally neither serializable nor
/// deserializable and contains no caller-supplied timestamp: it can only be
/// minted while the original review authority is still Running with one
/// durable Allow and before any authority output has been committed.
#[derive(Debug, Clone)]
pub struct ReviewRecoveryGrant {
    science_store: ScienceStore,
    operation_id: String,
    request_sha256: String,
    project_id: ProjectId,
    owner_id: String,
    session_id: String,
    source_run_id: String,
    authority_run_id: String,
    review_admission_sha256: String,
    source_authority_sha256: String,
    artifacts: Vec<ReviewedArtifact>,
    review_record_sha256: String,
    project_revision: String,
}

#[derive(Serialize)]
struct ReviewAdmissionDigest<'a> {
    schema: &'static str,
    request_sha256: &'a str,
    owner_id: &'a str,
    project_id: &'a ProjectId,
    session_id: &'a str,
    source_run_id: &'a str,
    authority_run_id: &'a str,
    source_authority_sha256: &'a str,
    artifacts: &'a [ReviewedArtifact],
    project_revision: &'a str,
}

impl ReviewAdmission {
    pub const ENV_ADMISSION_SHA256: &'static str = "review_admission_sha256";
    pub const ENV_REQUEST_SHA256: &'static str = "review_request_sha256";
    pub const ENV_SOURCE_AUTHORITY_SHA256: &'static str = "review_source_authority_sha256";
    pub const ENV_PROJECT_REVISION: &'static str = "review_project_revision";

    /// Capture one immutable review admission from retained store
    /// capabilities. The request's optional caller CAS remains part of its
    /// normalized fingerprint, but admission independently binds the actual
    /// current project revision even when `expected_revision` is `None`.
    pub fn capture(
        project_store: &ProjectStore,
        science_store: &ScienceStore,
        request: &super::mutation::MutationRequest,
    ) -> crate::Result<Self> {
        let ProjectMutationReviewFields { project_id, .. } =
            ProjectMutationReviewFields::from_request(request)?;
        project_store.with_owned_project_revision(
            project_id,
            &request.owner_id,
            |_project, revision| {
                project_store.capture_review_admission_inner(science_store, request, revision)
            },
        )
    }

    pub fn request_sha256(&self) -> &str {
        &self.request_sha256
    }

    pub fn owner_id(&self) -> &str {
        &self.owner_id
    }

    pub fn project_id(&self) -> &ProjectId {
        &self.project_id
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn source_run_id(&self) -> &str {
        &self.source_run_id
    }

    pub fn authority_run_id(&self) -> &str {
        &self.authority_run_id
    }

    pub fn source_authority_sha256(&self) -> &str {
        &self.source_authority_sha256
    }

    pub fn artifacts(&self) -> &[ReviewedArtifact] {
        &self.artifacts
    }

    pub fn project_revision(&self) -> &str {
        &self.project_revision
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Exact bindings that must be present in the actor authority
    /// `RunContext.environment` before its approval may authorize a review.
    pub fn authority_environment(&self) -> BTreeMap<String, String> {
        BTreeMap::from([
            (Self::ENV_ADMISSION_SHA256.into(), self.sha256().to_string()),
            (
                Self::ENV_REQUEST_SHA256.into(),
                self.request_sha256().to_string(),
            ),
            (
                Self::ENV_SOURCE_AUTHORITY_SHA256.into(),
                self.source_authority_sha256().to_string(),
            ),
            (
                Self::ENV_PROJECT_REVISION.into(),
                self.project_revision().to_string(),
            ),
        ])
    }

    /// Reopen every admission-bearing input after durable Allow while holding
    /// the project write lock. This is useful to the actor before it enters
    /// its finish phase; `apply_actor_review` performs the same check again and
    /// therefore does not trust callers to have invoked this method.
    pub fn verify_after_allow(
        &self,
        project_store: &ProjectStore,
        science_store: &ScienceStore,
        request: &super::mutation::MutationRequest,
    ) -> crate::Result<()> {
        let ProjectMutationReviewFields { project_id, .. } =
            ProjectMutationReviewFields::from_request(request)?;
        project_store.with_owned_project_revision(
            project_id,
            &request.owner_id,
            |_project, revision| {
                self.verify_after_allow_locked(project_store, science_store, request, revision)
            },
        )
    }

    pub(super) fn verify_after_allow_locked(
        &self,
        project_store: &ProjectStore,
        science_store: &ScienceStore,
        request: &super::mutation::MutationRequest,
        current_revision: &str,
    ) -> crate::Result<()> {
        self.verify_digest()?;
        let recaptured = project_store.capture_review_admission_inner(
            science_store,
            request,
            current_revision,
        )?;
        if recaptured != *self {
            return Err(ScienceError::Invalid(
                "review request, source authority, artifacts, or project revision changed after admission"
                    .into(),
            ));
        }
        project_store.verify_review_authority_after_allow(science_store, request, self)
    }

    fn new(
        request_sha256: String,
        owner_id: String,
        project_id: ProjectId,
        session_id: String,
        source_run_id: String,
        authority_run_id: String,
        source_authority_sha256: String,
        artifacts: Vec<ReviewedArtifact>,
        project_revision: String,
    ) -> crate::Result<Self> {
        let mut admission = Self {
            request_sha256,
            owner_id,
            project_id,
            session_id,
            source_run_id,
            authority_run_id,
            source_authority_sha256,
            artifacts,
            project_revision,
            sha256: String::new(),
        };
        admission.sha256 = admission.canonical_sha256()?;
        Ok(admission)
    }

    fn canonical_sha256(&self) -> crate::Result<String> {
        let canonical = ReviewAdmissionDigest {
            schema: "lumen-science-review-admission-v1",
            request_sha256: &self.request_sha256,
            owner_id: &self.owner_id,
            project_id: &self.project_id,
            session_id: &self.session_id,
            source_run_id: &self.source_run_id,
            authority_run_id: &self.authority_run_id,
            source_authority_sha256: &self.source_authority_sha256,
            artifacts: &self.artifacts,
            project_revision: &self.project_revision,
        };
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical)?)
        ))
    }

    fn verify_digest(&self) -> crate::Result<()> {
        if self.sha256 != self.canonical_sha256()? {
            return Err(ScienceError::Invalid(
                "review admission digest does not match its immutable bindings".into(),
            ));
        }
        Ok(())
    }
}

struct ProjectMutationReviewFields<'a> {
    project_id: &'a ProjectId,
    reviewer_id: &'a str,
    verdict: ReviewVerdict,
    summary: &'a str,
    claim_id: &'a Option<String>,
    source_run_id: &'a str,
    authority_run_id: &'a str,
    artifact_sha256s: &'a [String],
}

impl<'a> ProjectMutationReviewFields<'a> {
    fn from_request(request: &'a super::mutation::MutationRequest) -> crate::Result<Self> {
        let super::mutation::ProjectMutation::ReviewRecord {
            project_id,
            reviewer_id,
            verdict,
            summary,
            claim_id,
            source_run_id,
            authority_run_id,
            artifact_sha256s,
        } = &request.mutation
        else {
            return Err(ScienceError::Invalid(
                "review admission cannot authorize another mutation kind".into(),
            ));
        };
        Ok(Self {
            project_id,
            reviewer_id,
            verdict: *verdict,
            summary,
            claim_id,
            source_run_id,
            authority_run_id,
            artifact_sha256s,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewRecord {
    pub schema_version: u32,
    /// Equal to the mutation operation id. This makes a crash between the
    /// review write and operation-ledger write heal to the same file on retry
    /// instead of minting a duplicate review.
    pub review_id: String,
    pub operation_id: String,
    pub project_id: ProjectId,
    pub owner_id: String,
    pub session_id: String,
    pub reviewer_id: String,
    pub verdict: ReviewVerdict,
    pub summary: String,
    pub claim_id: Option<String>,
    pub source_run_id: String,
    /// SessionActor approval run that authorized and evidenced this record.
    pub authority_run_id: String,
    pub artifacts: Vec<ReviewedArtifact>,
    /// Exact digest of the source run's complete approval/artifact/evidence/
    /// provenance/preview/event authority snapshot at review admission.
    pub source_authority_sha256: String,
    /// Immutable pre-Allow admission and normalized mutation bindings.
    pub review_admission_sha256: String,
    pub review_request_sha256: String,
    pub review_project_revision: String,
    pub evidence_fingerprint: String,
    pub recorded_at: DateTime<Utc>,
}

impl ReviewRecord {
    pub fn expected_evidence(&self, manifest_sha256: String) -> crate::Evidence {
        crate::Evidence {
            run_id: RunId::new(&self.authority_run_id),
            claim: format!(
                "review {} records a verdict over {} rehashed artifact(s)",
                self.operation_id,
                self.artifacts.len()
            ),
            source: format!("project review ledger operation {}", self.operation_id),
            artifact_sha256: Some(manifest_sha256),
            verified_at: self.recorded_at,
        }
    }

    pub fn expected_provenance(&self) -> crate::Provenance {
        crate::Provenance {
            run_id: RunId::new(&self.authority_run_id),
            source_uri: format!("lumen-science://review/{}", self.operation_id),
            source_commit: None,
            source_path: None,
            license: "project-local-evidence".into(),
            retrieved_at: self.recorded_at,
            input_sha256: self.source_authority_sha256.clone(),
            tool: "SessionActor/review_record-v3".into(),
            environment: BTreeMap::from([
                ("source_run_id".into(), self.source_run_id.clone()),
                ("artifact_count".into(), self.artifacts.len().to_string()),
                (
                    "evidence_fingerprint".into(),
                    self.evidence_fingerprint.clone(),
                ),
                (
                    ReviewAdmission::ENV_ADMISSION_SHA256.into(),
                    self.review_admission_sha256.clone(),
                ),
                (
                    ReviewAdmission::ENV_REQUEST_SHA256.into(),
                    self.review_request_sha256.clone(),
                ),
                (
                    ReviewAdmission::ENV_SOURCE_AUTHORITY_SHA256.into(),
                    self.source_authority_sha256.clone(),
                ),
                (
                    ReviewAdmission::ENV_PROJECT_REVISION.into(),
                    self.review_project_revision.clone(),
                ),
                ("network".into(), "disabled".into()),
            ]),
        }
    }
}

impl ReviewRecoveryGrant {
    /// Verify and retain the exact orphaned review commit.
    ///
    /// This is the only constructor. The project write lock keeps the
    /// review-ledger read, operation-ledger absence check, and retained-root
    /// verification in one project mutation critical section.
    pub fn verify(
        project_store: &ProjectStore,
        science_store: &ScienceStore,
        request: &super::mutation::MutationRequest,
    ) -> crate::Result<Self> {
        let _guard = project_store.write_guard()?;
        let (record, project_revision) =
            Self::verify_orphan_record_locked(project_store, science_store, request)?;
        if project_store
            .lookup_operation(&request.operation_id)?
            .is_some()
        {
            return Err(ScienceError::Invalid(
                "review recovery requires an absent operation ledger record".into(),
            ));
        }
        Ok(Self {
            science_store: science_store.clone(),
            operation_id: request.operation_id.clone(),
            request_sha256: record.review_request_sha256.clone(),
            project_id: record.project_id.clone(),
            owner_id: record.owner_id.clone(),
            session_id: record.session_id.clone(),
            source_run_id: record.source_run_id.clone(),
            authority_run_id: record.authority_run_id.clone(),
            review_admission_sha256: record.review_admission_sha256.clone(),
            source_authority_sha256: record.source_authority_sha256.clone(),
            artifacts: record.artifacts.clone(),
            review_record_sha256: Self::record_sha256(&record)?,
            project_revision,
        })
    }

    pub(super) fn revalidate_locked(
        &self,
        project_store: &ProjectStore,
        request: &super::mutation::MutationRequest,
    ) -> crate::Result<(ReviewRecord, String)> {
        let (record, project_revision) =
            Self::verify_orphan_record_locked(project_store, &self.science_store, request)?;
        if self.operation_id != request.operation_id
            || self.request_sha256 != request.replay_fingerprint()?
            || self.request_sha256 != record.review_request_sha256
            || self.project_id != record.project_id
            || self.owner_id != record.owner_id
            || self.session_id != record.session_id
            || self.source_run_id != record.source_run_id
            || self.authority_run_id != record.authority_run_id
            || self.review_admission_sha256 != record.review_admission_sha256
            || self.source_authority_sha256 != record.source_authority_sha256
            || self.artifacts != record.artifacts
            || self.review_record_sha256 != Self::record_sha256(&record)?
            || self.project_revision != project_revision
        {
            return Err(ScienceError::Invalid(
                "review recovery grant no longer matches its request, ledger, source, authority, or project"
                    .into(),
            ));
        }
        Ok((record, project_revision))
    }

    fn verify_orphan_record_locked(
        project_store: &ProjectStore,
        science_store: &ScienceStore,
        request: &super::mutation::MutationRequest,
    ) -> crate::Result<(ReviewRecord, String)> {
        if !science_store.shares_root_capability_with(project_store)? {
            return Err(ScienceError::Invalid(
                "review recovery ScienceStore and ProjectStore retained different roots".into(),
            ));
        }
        let fields = ProjectMutationReviewFields::from_request(request)?;
        let record = project_store
            .lookup_review_record(fields.project_id, &request.operation_id)?
            .ok_or_else(|| {
                ScienceError::Invalid(format!("orphan review {} is missing", request.operation_id))
            })?;
        let request_sha256 = request.replay_fingerprint()?;
        let requested_artifacts: BTreeSet<_> =
            fields.artifact_sha256s.iter().map(String::as_str).collect();
        let recorded_artifacts: BTreeSet<_> = record
            .artifacts
            .iter()
            .map(|artifact| artifact.sha256.as_str())
            .collect();
        if record.schema_version != 3
            || record.operation_id != request.operation_id
            || record.review_id != request.operation_id
            || record.review_request_sha256 != request_sha256
            || record.project_id != *fields.project_id
            || record.owner_id != request.owner_id
            || record.session_id != request.session_id
            || record.reviewer_id != fields.reviewer_id
            || record.verdict != fields.verdict
            || record.summary != fields.summary
            || record.claim_id != *fields.claim_id
            || record.source_run_id != fields.source_run_id
            || record.authority_run_id != fields.authority_run_id
            || requested_artifacts.len() != fields.artifact_sha256s.len()
            || recorded_artifacts.len() != record.artifacts.len()
            || requested_artifacts != recorded_artifacts
            || record
                .artifacts
                .iter()
                .any(|artifact| artifact.source_run_id != record.source_run_id)
        {
            return Err(ScienceError::Invalid(
                "orphan review does not exactly match its original request".into(),
            ));
        }

        // This re-reads the operation-addressed ledger path and requires exact
        // equality, reconstructs the v3 admission, re-verifies the source
        // HostVerification and every selected artifact byte, then proves the
        // original authority is still Running with one Allow and all four
        // durable environment bindings.
        project_store.verify_pending_review_record_with_store(science_store, &record)?;
        let authority_run = RunId::new(&record.authority_run_id);
        if !science_store.artifacts(&authority_run)?.is_empty()
            || !science_store.evidence(&authority_run)?.is_empty()
            || !science_store.provenance(&authority_run)?.is_empty()
            || !science_store.previews(&authority_run)?.is_empty()
        {
            return Err(ScienceError::Invalid(
                "review recovery is only valid before authority outputs are committed".into(),
            ));
        }
        let project_revision = project_store.project_revision(&record.project_id)?;
        if project_revision == record.review_project_revision {
            return Err(ScienceError::Invalid(
                "orphan review is not reflected in the current project revision".into(),
            ));
        }
        Ok((record, project_revision))
    }

    fn record_sha256(record: &ReviewRecord) -> crate::Result<String> {
        Ok(crate::hex_sha256(&serde_json::to_vec(record)?))
    }
}

impl ProjectStore {
    fn validate_run_id(value: &str, field: &str) -> crate::Result<()> {
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

    fn reviews_relative(&self, project_id: &ProjectId) -> crate::Result<PathBuf> {
        super::model::validate_project_id(&project_id.0)?;
        Ok(PathBuf::from("projects")
            .join(&project_id.0)
            .join("reviews"))
    }

    fn review_relative(
        &self,
        project_id: &ProjectId,
        operation_id: &str,
    ) -> crate::Result<PathBuf> {
        super::mutation::validate_operation_id(operation_id)?;
        Ok(self
            .reviews_relative(project_id)?
            .join(format!("{operation_id}.json")))
    }

    #[cfg(test)]
    fn reviews_dir(&self, project_id: &ProjectId) -> PathBuf {
        self.project_dir(project_id).join("reviews")
    }

    #[cfg(test)]
    fn review_path(&self, project_id: &ProjectId, operation_id: &str) -> PathBuf {
        self.reviews_dir(project_id)
            .join(format!("{operation_id}.json"))
    }

    fn review_fingerprint(
        project_id: &ProjectId,
        source_run_id: &str,
        artifacts: &[ReviewedArtifact],
    ) -> String {
        let mut fingerprint = Sha256::new();
        fingerprint.update(b"lumen-science-review-evidence-v1\0");
        fingerprint.update(project_id.0.as_bytes());
        fingerprint.update([0]);
        fingerprint.update(source_run_id.as_bytes());
        fingerprint.update([0]);
        for artifact in artifacts {
            fingerprint.update(artifact.sha256.as_bytes());
            fingerprint.update([0]);
            fingerprint.update(artifact.relative_path.to_string_lossy().as_bytes());
            fingerprint.update([0]);
        }
        format!("{:x}", fingerprint.finalize())
    }

    fn capture_review_admission_inner(
        &self,
        science_store: &ScienceStore,
        request: &super::mutation::MutationRequest,
        current_revision: &str,
    ) -> crate::Result<ReviewAdmission> {
        self.gates().require(ScienceFeature::EvidenceGraph)?;
        self.gates().require(ScienceFeature::Collaboration)?;
        self.gates().require(ScienceFeature::ReviewPackage)?;
        super::mutation::validate_operation_id(&request.operation_id)?;
        if request.session_id.is_empty() || request.owner_id.is_empty() {
            return Err(ScienceError::Invalid(
                "review admission requires a session id and owner id".into(),
            ));
        }
        if let Some(expected) = request
            .expected_revision
            .as_deref()
            .filter(|expected| *expected != current_revision)
        {
            return Err(ScienceError::Invalid(format!(
                "revision conflict on review admission: expected {expected}, found {current_revision}"
            )));
        }
        if !science_store.shares_root_capability_with(self)? {
            return Err(ScienceError::Invalid(
                "review ScienceStore and ProjectStore retained different roots".into(),
            ));
        }

        let fields = ProjectMutationReviewFields::from_request(request)?;
        if fields.reviewer_id.trim().is_empty() || fields.reviewer_id.len() > 128 {
            return Err(ScienceError::Invalid(
                "reviewerId must be 1..=128 characters".into(),
            ));
        }
        if fields.reviewer_id != request.owner_id {
            return Err(ScienceError::Ownership);
        }
        if fields.summary.trim().is_empty() || fields.summary.len() > 16_384 {
            return Err(ScienceError::Invalid(
                "review summary must be 1..=16384 characters".into(),
            ));
        }
        Self::validate_run_id(fields.source_run_id, "runId")?;
        Self::validate_run_id(fields.authority_run_id, "authority run id")?;
        if fields.artifact_sha256s.is_empty() || fields.artifact_sha256s.len() > 128 {
            return Err(ScienceError::Invalid(
                "review requires 1..=128 artifact SHA-256 values".into(),
            ));
        }

        let project = self.load_project(fields.project_id)?;
        if project.owner_id.0 != request.owner_id {
            return Err(ScienceError::Ownership);
        }
        if let Some(claim_id) = fields.claim_id.as_deref() {
            let claim = self.load_claim(fields.project_id, claim_id)?;
            if claim.project_id != *fields.project_id {
                return Err(ScienceError::Ownership);
            }
        }

        let mut requested = BTreeSet::new();
        for sha256 in fields.artifact_sha256s {
            validate_sha256_hex(sha256).map_err(ScienceError::Invalid)?;
            if !requested.insert(sha256.clone()) {
                return Err(ScienceError::Invalid(
                    "review artifact SHA-256 values must be unique".into(),
                ));
            }
        }

        let run_id = RunId::new(fields.source_run_id);
        let run = science_store.load_run(&run_id)?;
        let science_project = crate::ProjectId::new(fields.project_id.0.clone());
        if run.state != RunState::Succeeded {
            return Err(ScienceError::Invalid(
                "review artifacts must come from a succeeded run".into(),
            ));
        }
        if run.context.project_id != science_project
            || run.context.owner_id != request.owner_id
            || run.context.session_id != request.session_id
            || run.context.artifact_root != self.root().join("runs")
        {
            return Err(ScienceError::Ownership);
        }

        let source_authority = crate::review::verify_for_goal_completion(science_store, &run_id)?;
        let mut registered = science_store.artifacts(&run_id)?;
        registered.sort_by(|left, right| {
            left.sha256
                .cmp(&right.sha256)
                .then_with(|| left.relative_path.cmp(&right.relative_path))
        });
        let mut reviewed = Vec::with_capacity(requested.len());
        for sha256 in requested {
            let matches: Vec<_> = registered
                .iter()
                .filter(|artifact| artifact.sha256 == sha256)
                .collect();
            let artifact = match matches.as_slice() {
                [artifact] => *artifact,
                [] => {
                    return Err(ScienceError::Invalid(format!(
                        "artifact {sha256} is not registered in source run {}",
                        fields.source_run_id
                    )));
                }
                _ => {
                    return Err(ScienceError::Invalid(format!(
                        "artifact {sha256} is ambiguous in source run {}",
                        fields.source_run_id
                    )));
                }
            };
            let bytes = science_store.artifact_bytes(
                &science_project,
                &run_id,
                &request.owner_id,
                &artifact.relative_path,
            )?;
            if bytes.len() as u64 != artifact.bytes || crate::hex_sha256(&bytes) != artifact.sha256
            {
                return Err(ScienceError::Invalid(
                    "review artifact bytes do not match their registered hash/length".into(),
                ));
            }
            reviewed.push(ReviewedArtifact {
                source_run_id: fields.source_run_id.to_string(),
                relative_path: artifact.relative_path.clone(),
                sha256: artifact.sha256.clone(),
                bytes: artifact.bytes,
                mime: artifact.mime.clone(),
            });
        }
        reviewed.sort_by(|left, right| {
            left.sha256
                .cmp(&right.sha256)
                .then_with(|| left.relative_path.cmp(&right.relative_path))
        });

        ReviewAdmission::new(
            request.replay_fingerprint()?,
            request.owner_id.clone(),
            fields.project_id.clone(),
            request.session_id.clone(),
            fields.source_run_id.to_string(),
            fields.authority_run_id.to_string(),
            source_authority.verification_sha256,
            reviewed,
            current_revision.to_string(),
        )
    }

    fn verify_review_authority_after_allow(
        &self,
        science_store: &ScienceStore,
        request: &super::mutation::MutationRequest,
        admission: &ReviewAdmission,
    ) -> crate::Result<()> {
        let fields = ProjectMutationReviewFields::from_request(request)?;
        if admission.request_sha256() != request.replay_fingerprint()?
            || admission.owner_id() != request.owner_id
            || admission.project_id() != fields.project_id
            || admission.session_id() != request.session_id
            || admission.source_run_id() != fields.source_run_id
            || admission.authority_run_id() != fields.authority_run_id
        {
            return Err(ScienceError::Ownership);
        }

        let source_id = RunId::new(fields.source_run_id);
        let source = science_store.load_run(&source_id)?;
        let authority_id = RunId::new(fields.authority_run_id);
        let authority = science_store.load_run(&authority_id)?;
        let science_project = crate::ProjectId::new(fields.project_id.0.clone());
        if authority.state != RunState::Running
            || authority.context.project_id != science_project
            || authority.context.owner_id != request.owner_id
            || authority.context.session_id != request.session_id
            || authority.context.workspace_root != source.context.workspace_root
            || authority.context.artifact_root != source.context.artifact_root
            || authority.context.artifact_root != self.root().join("runs")
        {
            return Err(ScienceError::Ownership);
        }
        for (key, expected) in admission.authority_environment() {
            if authority.context.environment.get(&key) != Some(&expected) {
                return Err(ScienceError::Invalid(format!(
                    "review authority context is missing exact {key} binding"
                )));
            }
        }
        let approvals = science_store.approvals(&authority_id)?;
        let [approval] = approvals.as_slice() else {
            return Err(ScienceError::Invalid(
                "review authority run requires exactly one terminal Allow approval".into(),
            ));
        };
        if approval.project_id != science_project
            || approval.run_id != authority_id
            || approval.call_id != crate::CallId::new("science_project_mutation")
            || approval.owner_id != request.owner_id
            || approval.decision != crate::ApprovalDecision::Allow
            || approval.decided_at.is_none()
        {
            return Err(ScienceError::Invalid(
                "review authority approval does not exactly bind its mutation call".into(),
            ));
        }
        Ok(())
    }

    pub fn list_reviews_with_store(
        &self,
        science_store: &ScienceStore,
        project_id: &ProjectId,
    ) -> crate::Result<Vec<ReviewRecord>> {
        if !science_store.shares_root_capability_with(self)? {
            return Err(ScienceError::Invalid(
                "review ScienceStore and ProjectStore retained different roots".into(),
            ));
        }
        self.gates().require(ScienceFeature::Collaboration)?;
        self.gates().require(ScienceFeature::ReviewPackage)?;
        let project = self.load_project(project_id)?;
        let dir = self.reviews_relative(project_id)?;
        let mut records = Vec::new();
        for name in self.list_confined(&dir)? {
            let path = dir.join(name);
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let record: ReviewRecord = self.read_confined_record(&path)?.ok_or_else(|| {
                ScienceError::Invalid("review ledger entry disappeared during listing".into())
            })?;
            if record.project_id != *project_id || record.owner_id != project.owner_id.0 {
                return Err(ScienceError::Ownership);
            }
            self.verify_review_record_with_store(science_store, &record)?;
            records.push(record);
        }
        records.sort_by(|left, right| {
            left.recorded_at
                .cmp(&right.recorded_at)
                .then_with(|| left.review_id.cmp(&right.review_id))
        });
        Ok(records)
    }

    /// Read an operation-addressed review ledger entry without assuming its
    /// operation record was durably written. The SessionActor uses this to
    /// recover the narrow crash window between the two atomic files.
    pub fn lookup_review_record(
        &self,
        project_id: &ProjectId,
        operation_id: &str,
    ) -> crate::Result<Option<ReviewRecord>> {
        super::mutation::validate_operation_id(operation_id)?;
        super::model::validate_project_id(&project_id.0)?;
        let path = self.review_relative(project_id, operation_id)?;
        let Some(record): Option<ReviewRecord> = self.read_confined_record(&path)? else {
            return Ok(None);
        };
        if record.project_id != *project_id || record.operation_id != operation_id {
            return Err(ScienceError::Ownership);
        }
        Ok(Some(record))
    }

    pub(super) fn record_review_inner(
        &self,
        science_store: &ScienceStore,
        request: &super::mutation::MutationRequest,
        admission: &ReviewAdmission,
        current_revision: &str,
    ) -> crate::Result<ReviewRecord> {
        admission.verify_after_allow_locked(self, science_store, request, current_revision)?;
        let fields = ProjectMutationReviewFields::from_request(request)?;
        let evidence_fingerprint = Self::review_fingerprint(
            fields.project_id,
            fields.source_run_id,
            admission.artifacts(),
        );
        let record = ReviewRecord {
            schema_version: 3,
            review_id: request.operation_id.clone(),
            operation_id: request.operation_id.clone(),
            project_id: fields.project_id.clone(),
            owner_id: request.owner_id.clone(),
            session_id: request.session_id.clone(),
            reviewer_id: fields.reviewer_id.to_string(),
            verdict: fields.verdict,
            summary: fields.summary.to_string(),
            claim_id: fields.claim_id.clone(),
            source_run_id: fields.source_run_id.to_string(),
            authority_run_id: fields.authority_run_id.to_string(),
            artifacts: admission.artifacts().to_vec(),
            source_authority_sha256: admission.source_authority_sha256().to_string(),
            review_admission_sha256: admission.sha256().to_string(),
            review_request_sha256: admission.request_sha256().to_string(),
            review_project_revision: admission.project_revision().to_string(),
            evidence_fingerprint,
            recorded_at: Utc::now(),
        };

        let path = self.review_relative(fields.project_id, &request.operation_id)?;
        if let Some(existing) = self.read_confined_record::<ReviewRecord>(&path)? {
            if existing.operation_id == record.operation_id
                && existing.project_id == record.project_id
                && existing.owner_id == record.owner_id
                && existing.session_id == record.session_id
                && existing.reviewer_id == record.reviewer_id
                && existing.verdict == record.verdict
                && existing.summary == record.summary
                && existing.claim_id == record.claim_id
                && existing.source_run_id == record.source_run_id
                && existing.authority_run_id == record.authority_run_id
                && existing.artifacts == record.artifacts
                && existing.source_authority_sha256 == record.source_authority_sha256
                && existing.review_admission_sha256 == record.review_admission_sha256
                && existing.review_request_sha256 == record.review_request_sha256
                && existing.review_project_revision == record.review_project_revision
                && existing.evidence_fingerprint == record.evidence_fingerprint
            {
                return Ok(existing);
            }
            return Err(ScienceError::Invalid(format!(
                "review operation {} already exists with different content",
                request.operation_id
            )));
        }
        self.write_new_confined_record(&path, &record)?;
        Ok(record)
    }

    fn verify_review_record_binding(
        &self,
        science_store: &ScienceStore,
        record: &ReviewRecord,
        authority_state: RunState,
    ) -> crate::Result<RunId> {
        super::mutation::validate_operation_id(&record.operation_id)?;
        Self::validate_run_id(&record.source_run_id, "source run id")?;
        Self::validate_run_id(&record.authority_run_id, "authority run id")?;
        if record.schema_version != 3
            || record.review_id != record.operation_id
            || record.reviewer_id.trim().is_empty()
            || record.reviewer_id.len() > 128
            || record.summary.trim().is_empty()
            || record.summary.len() > 16_384
            || record.artifacts.is_empty()
            || record.artifacts.len() > 128
        {
            return Err(ScienceError::Invalid(
                "project review record has invalid identity or content bounds".into(),
            ));
        }
        validate_sha256_hex(&record.source_authority_sha256).map_err(ScienceError::Invalid)?;
        validate_sha256_hex(&record.review_admission_sha256).map_err(ScienceError::Invalid)?;
        validate_sha256_hex(&record.review_request_sha256).map_err(ScienceError::Invalid)?;
        validate_sha256_hex(&record.review_project_revision).map_err(ScienceError::Invalid)?;
        validate_sha256_hex(&record.evidence_fingerprint).map_err(ScienceError::Invalid)?;
        let recorded_admission = ReviewAdmission::new(
            record.review_request_sha256.clone(),
            record.owner_id.clone(),
            record.project_id.clone(),
            record.session_id.clone(),
            record.source_run_id.clone(),
            record.authority_run_id.clone(),
            record.source_authority_sha256.clone(),
            record.artifacts.clone(),
            record.review_project_revision.clone(),
        )?;
        if recorded_admission.sha256() != record.review_admission_sha256 {
            return Err(ScienceError::Invalid(
                "project review admission digest does not match its durable bindings".into(),
            ));
        }
        let project = self.load_project(&record.project_id)?;
        if project.owner_id.0 != record.owner_id || record.reviewer_id != record.owner_id {
            return Err(ScienceError::Ownership);
        }
        let ledger_path = self.review_relative(&record.project_id, &record.operation_id)?;
        let ledger_record: ReviewRecord =
            self.read_confined_record(&ledger_path)?.ok_or_else(|| {
                ScienceError::Invalid("project review ledger entry is missing".into())
            })?;
        if ledger_record != *record {
            return Err(ScienceError::Invalid(
                "project review ledger does not match its operation result".into(),
            ));
        }
        if !science_store.shares_root_capability_with(self)? {
            return Err(ScienceError::Invalid(
                "review ScienceStore and ProjectStore retained different roots".into(),
            ));
        }
        let source_run = RunId::new(&record.source_run_id);
        let source = science_store.load_run(&source_run)?;
        if source.state != RunState::Succeeded
            || source.context.project_id.0 != record.project_id.0
            || source.context.owner_id != record.owner_id
            || source.context.session_id != record.session_id
        {
            return Err(ScienceError::Ownership);
        }
        let source_authority =
            crate::review::verify_for_goal_completion(science_store, &source_run)?;
        if source_authority.verification_sha256 != record.source_authority_sha256 {
            return Err(ScienceError::Invalid(
                "review source authority snapshot no longer matches its admitted proof".into(),
            ));
        }
        if source.context.artifact_root != self.root().join("runs") {
            return Err(ScienceError::Invalid(
                "review source run is outside its bound workspace/store root".into(),
            ));
        }
        let registered = science_store.artifacts(&source_run)?;
        let mut seen = BTreeSet::new();
        for artifact in &record.artifacts {
            validate_sha256_hex(&artifact.sha256).map_err(ScienceError::Invalid)?;
            if artifact.source_run_id != record.source_run_id
                || !seen.insert(artifact.sha256.clone())
            {
                return Err(ScienceError::Ownership);
            }
            let matches: Vec<_> = registered
                .iter()
                .filter(|candidate| candidate.sha256 == artifact.sha256)
                .collect();
            let durable = match matches.as_slice() {
                [durable] => *durable,
                [] => {
                    return Err(ScienceError::Invalid(
                        "reviewed artifact is absent from its source-run registry".into(),
                    ));
                }
                _ => {
                    return Err(ScienceError::Invalid(
                        "reviewed artifact is ambiguous in its source-run registry".into(),
                    ));
                }
            };
            if durable.relative_path != artifact.relative_path
                || durable.bytes != artifact.bytes
                || durable.mime != artifact.mime
            {
                return Err(ScienceError::Invalid(
                    "reviewed artifact metadata does not match its source-run registry".into(),
                ));
            }
            let bytes = science_store.artifact_bytes(
                &source.context.project_id,
                &source_run,
                &record.owner_id,
                &artifact.relative_path,
            )?;
            if bytes.len() as u64 != artifact.bytes || crate::hex_sha256(&bytes) != artifact.sha256
            {
                return Err(ScienceError::Invalid(
                    "reviewed source artifact no longer matches its durable record".into(),
                ));
            }
        }
        if Self::review_fingerprint(&record.project_id, &record.source_run_id, &record.artifacts)
            != record.evidence_fingerprint
        {
            return Err(ScienceError::Invalid(
                "review evidence fingerprint does not match its artifact set".into(),
            ));
        }
        let authority_run = RunId::new(&record.authority_run_id);
        let authority = science_store.load_run(&authority_run)?;
        if authority.state != authority_state
            || authority.context.project_id.0 != record.project_id.0
            || authority.context.owner_id != record.owner_id
            || authority.context.session_id != record.session_id
        {
            return Err(ScienceError::Ownership);
        }
        if authority.context.workspace_root != source.context.workspace_root
            || authority.context.artifact_root != source.context.artifact_root
        {
            return Err(ScienceError::Invalid(
                "review authority run is outside its source-run workspace/store".into(),
            ));
        }
        for (key, expected) in recorded_admission.authority_environment() {
            if authority.context.environment.get(&key) != Some(&expected) {
                return Err(ScienceError::Invalid(format!(
                    "review authority context no longer matches exact {key} binding"
                )));
            }
        }
        let approvals = science_store.approvals(&authority_run)?;
        let [approval] = approvals.as_slice() else {
            return Err(ScienceError::Invalid(
                "review authority run must have exactly one approval".into(),
            ));
        };
        if approval.project_id.0 != record.project_id.0
            || approval.run_id != authority_run
            || approval.call_id != crate::CallId::new("science_project_mutation")
            || approval.owner_id != record.owner_id
            || approval.decision != crate::ApprovalDecision::Allow
            || approval.decided_at.is_none()
        {
            return Err(ScienceError::Invalid(
                "review authority approval does not exactly bind its mutation call".into(),
            ));
        }
        Ok(authority_run)
    }

    /// Validate the durable project + operation half of a review commit while
    /// its already-Allowed authority run is still Running. The SessionActor
    /// uses this only to recover an interrupted evidence commit; callers
    /// cannot turn a denied/failed run back into authority.
    /// Retained-capability variant used by the SessionActor after permission.
    pub fn verify_pending_review_record_with_store(
        &self,
        science_store: &ScienceStore,
        record: &ReviewRecord,
    ) -> crate::Result<()> {
        self.verify_review_record_binding(science_store, record, RunState::Running)?;
        Ok(())
    }

    /// Verify the complete manifest/evidence/provenance commit while the
    /// authority run is still Running. The SessionActor calls this before its
    /// final Succeeded transition.
    /// Retained-capability variant used by the SessionActor before Succeeded.
    pub fn verify_pending_review_commit_with_store(
        &self,
        science_store: &ScienceStore,
        record: &ReviewRecord,
    ) -> crate::Result<()> {
        self.verify_review_commit(science_store, record, RunState::Running)
    }

    /// Fail closed unless the review's own authority run is complete and its
    /// manifest/evidence/provenance exactly bind the project-ledger record.
    /// Retained-capability variant used for actor-owned replay verification.
    pub fn verify_review_record_with_store(
        &self,
        science_store: &ScienceStore,
        record: &ReviewRecord,
    ) -> crate::Result<()> {
        self.verify_review_commit(science_store, record, RunState::Succeeded)
    }

    fn verify_review_commit(
        &self,
        science_store: &ScienceStore,
        record: &ReviewRecord,
        authority_state: RunState,
    ) -> crate::Result<()> {
        let authority_run =
            self.verify_review_record_binding(science_store, record, authority_state)?;
        if authority_state == RunState::Running {
            crate::review::verify_before_successful_commit(science_store, &authority_run)?;
        } else {
            crate::review::verify_for_goal_completion(science_store, &authority_run)?;
        }
        let artifacts = science_store.artifacts(&authority_run)?;
        let [manifest] = artifacts.as_slice() else {
            return Err(ScienceError::Invalid(
                "review authority run must contain exactly one manifest artifact".into(),
            ));
        };
        if manifest.run_id != authority_run
            || manifest.call_id != crate::CallId::new("science_project_mutation")
            || manifest.relative_path != std::path::Path::new("review_record.json")
            || manifest.mime != "application/json"
            || manifest.preview != "actor-owned durable review record"
        {
            return Err(ScienceError::Invalid(
                "review authority run has no canonical review manifest metadata".into(),
            ));
        }
        let project_id = crate::ProjectId::new(record.project_id.0.clone());
        let manifest_bytes = if authority_state == RunState::Running {
            science_store.running_artifact_bytes(
                &project_id,
                &authority_run,
                &record.owner_id,
                &manifest.relative_path,
            )?
        } else {
            science_store.artifact_bytes(
                &project_id,
                &authority_run,
                &record.owner_id,
                &manifest.relative_path,
            )?
        };
        let manifest_record: ReviewRecord = serde_json::from_slice(&manifest_bytes)?;
        if manifest_record != *record {
            return Err(ScienceError::Invalid(
                "project review record does not match its authority manifest".into(),
            ));
        }
        let evidence = science_store.evidence(&authority_run)?;
        if evidence != vec![record.expected_evidence(manifest.sha256.clone())] {
            return Err(ScienceError::Invalid(
                "review evidence does not exactly bind its manifest and operation".into(),
            ));
        }
        let provenance = science_store.provenance(&authority_run)?;
        if provenance != vec![record.expected_provenance()] {
            return Err(ScienceError::Invalid(
                "review provenance does not exactly bind its source fingerprint".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn verify_review_replay_admission(
        &self,
        science_store: &ScienceStore,
        request: &super::mutation::MutationRequest,
        admission: &ReviewAdmission,
        record: &ReviewRecord,
    ) -> crate::Result<()> {
        admission.verify_digest()?;
        let fields = ProjectMutationReviewFields::from_request(request)?;
        if admission.request_sha256() != request.replay_fingerprint()?
            || admission.owner_id() != request.owner_id
            || admission.project_id() != fields.project_id
            || admission.session_id() != request.session_id
            || admission.source_run_id() != fields.source_run_id
            || admission.authority_run_id() != fields.authority_run_id
            || record.project_id != *admission.project_id()
            || record.owner_id != admission.owner_id()
            || record.session_id != admission.session_id()
            || record.source_run_id != admission.source_run_id()
            || record.authority_run_id != admission.authority_run_id()
            || record.artifacts != admission.artifacts()
            || record.source_authority_sha256 != admission.source_authority_sha256()
            || record.review_admission_sha256 != admission.sha256()
            || record.review_request_sha256 != admission.request_sha256()
            || record.review_project_revision != admission.project_revision()
        {
            return Err(ScienceError::Invalid(
                "review replay does not match its immutable admission".into(),
            ));
        }
        // This recomputes the current HostVerification v2 authority digest and
        // rehashes every reviewed artifact, so a format-valid replacement
        // after admission cannot replay an old operation.
        self.verify_review_record_with_store(science_store, record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CallId, ProjectId as ScienceProjectId, RunContext,
        project::{MutationRequest, ProjectMutation},
    };
    use std::{collections::BTreeMap, path::Path};

    struct Fixture {
        _root: tempfile::TempDir,
        store_root: PathBuf,
        science_store: ScienceStore,
        project_store: ProjectStore,
        project_id: ProjectId,
        run_id: RunId,
        authority_run_id: RunId,
        artifact_sha256: String,
    }

    fn fixture() -> Fixture {
        let root = tempfile::tempdir().unwrap();
        let workspace = dunce::canonicalize(root.path()).unwrap();
        let store_root = workspace.join("science-store");
        let project_store = ProjectStore::new(&store_root);
        let project = project_store
            .create_project("owner-1", "Review fixture", "Are these bytes intact?")
            .unwrap();
        let science_store = ScienceStore::new(&store_root);
        let run_id = RunId::new("source-run-1");
        science_store
            .create_run(RunContext {
                run_id: run_id.clone(),
                project_id: ScienceProjectId::new(project.project_id.0.clone()),
                session_id: "session-1".into(),
                owner_id: "owner-1".into(),
                workspace_root: workspace,
                provider: "offline-test".into(),
                approval_policy: "test".into(),
                tool_profile: "review-source-fixture".into(),
                artifact_root: store_root.join("runs"),
                environment: BTreeMap::new(),
            })
            .unwrap();
        let source_project_id = ScienceProjectId::new(project.project_id.0.clone());
        let source_call = CallId::new("source-call");
        science_store
            .request_approval(crate::Approval {
                project_id: source_project_id.clone(),
                run_id: run_id.clone(),
                call_id: source_call.clone(),
                owner_id: "owner-1".into(),
                decision: crate::ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        science_store
            .transition(&run_id, RunState::AwaitingApproval, None)
            .unwrap();
        science_store
            .decide_approval(
                &source_project_id,
                &run_id,
                "owner-1",
                &source_call,
                crate::ApprovalDecision::Allow,
            )
            .unwrap();
        science_store
            .transition(&run_id, RunState::Running, None)
            .unwrap();
        let artifact = science_store
            .put_artifact(
                &source_project_id,
                &run_id,
                "owner-1",
                source_call,
                Path::new("result.json"),
                br#"{"result":"verified"}"#,
                "application/json",
                "source result",
            )
            .unwrap();
        science_store
            .add_evidence(crate::Evidence {
                run_id: run_id.clone(),
                claim: "Source result bytes passed deterministic verification.".into(),
                source: "fixture://review-source/result.json".into(),
                artifact_sha256: Some(artifact.sha256.clone()),
                verified_at: Utc::now(),
            })
            .unwrap();
        science_store
            .add_provenance(crate::Provenance {
                run_id: run_id.clone(),
                source_uri: "fixture://review-source".into(),
                source_commit: None,
                source_path: Some("result.json".into()),
                license: "test-only".into(),
                retrieved_at: Utc::now(),
                input_sha256: artifact.sha256.clone(),
                tool: "review-source-fixture".into(),
                environment: BTreeMap::from([("network".into(), "disabled".into())]),
            })
            .unwrap();
        science_store
            .transition_succeeded_verified(&run_id)
            .unwrap();
        let authority_run_id = RunId::new("authority-run-1");
        science_store
            .create_run(RunContext {
                run_id: authority_run_id.clone(),
                project_id: ScienceProjectId::new(project.project_id.0.clone()),
                session_id: "session-1".into(),
                owner_id: "owner-1".into(),
                workspace_root: dunce::canonicalize(root.path()).unwrap(),
                provider: "offline-test".into(),
                approval_policy: "test".into(),
                tool_profile: "review-authority-fixture".into(),
                artifact_root: store_root.join("runs"),
                environment: BTreeMap::new(),
            })
            .unwrap();
        let authority_call = CallId::new("science_project_mutation");
        science_store
            .request_approval(crate::Approval {
                project_id: ScienceProjectId::new(project.project_id.0.clone()),
                run_id: authority_run_id.clone(),
                call_id: authority_call.clone(),
                owner_id: "owner-1".into(),
                decision: crate::ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        science_store
            .transition(&authority_run_id, RunState::AwaitingApproval, None)
            .unwrap();
        science_store
            .decide_approval(
                &ScienceProjectId::new(project.project_id.0.clone()),
                &authority_run_id,
                "owner-1",
                &authority_call,
                crate::ApprovalDecision::Allow,
            )
            .unwrap();
        science_store
            .transition(&authority_run_id, RunState::Running, None)
            .unwrap();
        Fixture {
            _root: root,
            store_root,
            science_store,
            project_store,
            project_id: project.project_id,
            run_id,
            authority_run_id,
            artifact_sha256: artifact.sha256,
        }
    }

    fn review_request(fixture: &Fixture, operation_id: &str) -> MutationRequest {
        MutationRequest {
            operation_id: operation_id.into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            expected_revision: Some(
                fixture
                    .project_store
                    .project_revision(&fixture.project_id)
                    .unwrap(),
            ),
            mutation: ProjectMutation::ReviewRecord {
                project_id: fixture.project_id.clone(),
                reviewer_id: "owner-1".into(),
                verdict: ReviewVerdict::Pass,
                summary: "The exact result bytes support the stated fixture conclusion.".into(),
                claim_id: None,
                source_run_id: fixture.run_id.0.clone(),
                authority_run_id: fixture.authority_run_id.0.clone(),
                artifact_sha256s: vec![fixture.artifact_sha256.clone()],
            },
        }
    }

    fn apply_review(
        fixture: &Fixture,
        request: &MutationRequest,
    ) -> crate::Result<crate::project::MutationOutcome> {
        let admission = admit_review(fixture, request)?;
        fixture
            .project_store
            .apply_actor_review(request, &fixture.science_store, &admission)
    }

    fn admit_review(
        fixture: &Fixture,
        request: &MutationRequest,
    ) -> crate::Result<ReviewAdmission> {
        let admission =
            ReviewAdmission::capture(&fixture.project_store, &fixture.science_store, request)?;
        let mut authority = fixture.science_store.load_run(&fixture.authority_run_id)?;
        authority.context.environment = admission.authority_environment();
        ProjectStore::write_json(
            &fixture
                .store_root
                .join("runs")
                .join(&fixture.authority_run_id.0)
                .join("run.json"),
            &authority,
        )?;
        admission.verify_after_allow(&fixture.project_store, &fixture.science_store, request)?;
        Ok(admission)
    }

    fn complete_authority_run(fixture: &Fixture, record: &ReviewRecord) {
        let science_store = &fixture.science_store;
        let manifest = serde_json::to_vec_pretty(record).unwrap();
        let artifact = science_store
            .put_artifact(
                &ScienceProjectId::new(fixture.project_id.0.clone()),
                &fixture.authority_run_id,
                "owner-1",
                CallId::new("science_project_mutation"),
                Path::new("review_record.json"),
                &manifest,
                "application/json",
                "actor-owned durable review record",
            )
            .unwrap();
        science_store
            .add_evidence(record.expected_evidence(artifact.sha256))
            .unwrap();
        science_store
            .add_provenance(record.expected_provenance())
            .unwrap();
        fixture
            .project_store
            .verify_pending_review_commit_with_store(science_store, record)
            .unwrap();
        science_store
            .transition_succeeded_verified(&fixture.authority_run_id)
            .unwrap();
    }

    fn assert_no_review_commit(fixture: &Fixture, operation_id: &str) {
        assert!(
            fixture
                .project_store
                .lookup_review_record(&fixture.project_id, operation_id)
                .unwrap()
                .is_none(),
            "rejected review wrote a review ledger record"
        );
        assert!(
            fixture
                .project_store
                .lookup_operation(operation_id)
                .unwrap()
                .is_none(),
            "rejected review wrote an operation record"
        );
    }

    fn write_orphan_review(fixture: &Fixture, request: &MutationRequest) -> ReviewRecord {
        let admission = admit_review(fixture, request).unwrap();
        let _guard = fixture.project_store.write_guard().unwrap();
        let revision = fixture
            .project_store
            .project_revision(&fixture.project_id)
            .unwrap();
        let record = fixture
            .project_store
            .record_review_inner(&fixture.science_store, request, &admission, &revision)
            .unwrap();
        assert!(
            fixture
                .project_store
                .lookup_operation(&request.operation_id)
                .unwrap()
                .is_none(),
            "orphan fixture unexpectedly wrote an operation record"
        );
        record
    }

    fn assert_operation_absent(fixture: &Fixture, operation_id: &str) {
        assert!(
            fixture
                .project_store
                .lookup_operation(operation_id)
                .unwrap()
                .is_none(),
            "rejected recovery wrote an operation record"
        );
    }

    #[test]
    fn review_orphan_recovery_writes_only_operation_and_replays_exactly() {
        let fixture = fixture();
        let request = review_request(&fixture, "op-review-orphan-recovery");
        let review = write_orphan_review(&fixture, &request);
        let review_path = fixture
            .project_store
            .review_path(&fixture.project_id, &request.operation_id);
        let review_bytes = std::fs::read(&review_path).unwrap();
        let expected_revision = fixture
            .project_store
            .project_revision(&fixture.project_id)
            .unwrap();
        let grant =
            ReviewRecoveryGrant::verify(&fixture.project_store, &fixture.science_store, &request)
                .unwrap();

        let outcome = fixture
            .project_store
            .recover_actor_review_operation(&request, &grant)
            .unwrap();
        assert!(outcome.replayed);
        assert_eq!(outcome.revision, expected_revision);
        assert_eq!(
            serde_json::from_value::<ReviewRecord>(outcome.result.clone()).unwrap(),
            review
        );
        assert_eq!(
            std::fs::read(&review_path).unwrap(),
            review_bytes,
            "recovery rewrote the durable review ledger"
        );

        let replay = fixture
            .project_store
            .recover_actor_review_operation(&request, &grant)
            .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay, outcome);
        assert!(
            ReviewRecoveryGrant::verify(&fixture.project_store, &fixture.science_store, &request)
                .is_err(),
            "a second grant was minted after the operation ledger existed"
        );

        let mut loosened = request.clone();
        loosened.expected_revision = None;
        assert!(
            fixture
                .project_store
                .recover_actor_review_operation(&loosened, &grant)
                .is_err(),
            "clearing the original expected revision replayed the orphan"
        );
    }

    #[test]
    fn review_orphan_recovery_grant_cannot_cross_request_source_or_root() {
        let fixture = fixture();
        let request = review_request(&fixture, "op-review-recovery-binding");
        write_orphan_review(&fixture, &request);
        let grant =
            ReviewRecoveryGrant::verify(&fixture.project_store, &fixture.science_store, &request)
                .unwrap();

        let mut changed_request = request.clone();
        if let ProjectMutation::ReviewRecord { summary, .. } = &mut changed_request.mutation {
            *summary = "different request".into();
        }
        assert!(
            fixture
                .project_store
                .recover_actor_review_operation(&changed_request, &grant)
                .is_err()
        );

        let mut changed_source = request.clone();
        if let ProjectMutation::ReviewRecord { source_run_id, .. } = &mut changed_source.mutation {
            *source_run_id = "different-source-run".into();
        }
        assert!(
            fixture
                .project_store
                .recover_actor_review_operation(&changed_source, &grant)
                .is_err()
        );

        let other = tempfile::tempdir().unwrap();
        let other_project_store = ProjectStore::new(other.path().join("science-store"));
        assert!(
            other_project_store
                .recover_actor_review_operation(&request, &grant)
                .is_err(),
            "a retained recovery grant crossed into another store root"
        );
        assert_operation_absent(&fixture, &request.operation_id);
    }

    #[test]
    fn review_orphan_recovery_rejects_project_source_authority_and_ledger_tamper() {
        let project_fixture = fixture();
        let project_request = review_request(&project_fixture, "op-review-recovery-project-tamper");
        write_orphan_review(&project_fixture, &project_request);
        let project_grant = ReviewRecoveryGrant::verify(
            &project_fixture.project_store,
            &project_fixture.science_store,
            &project_request,
        )
        .unwrap();
        let mut project = project_fixture
            .project_store
            .load_project(&project_fixture.project_id)
            .unwrap();
        project.title = "tampered project".into();
        ProjectStore::write_json(
            &project_fixture
                .project_store
                .project_dir(&project_fixture.project_id)
                .join("project.json"),
            &project,
        )
        .unwrap();
        assert!(
            project_fixture
                .project_store
                .recover_actor_review_operation(&project_request, &project_grant)
                .is_err()
        );
        assert_operation_absent(&project_fixture, &project_request.operation_id);

        let source_fixture = fixture();
        let source_request = review_request(&source_fixture, "op-review-recovery-source-tamper");
        write_orphan_review(&source_fixture, &source_request);
        let source_grant = ReviewRecoveryGrant::verify(
            &source_fixture.project_store,
            &source_fixture.science_store,
            &source_request,
        )
        .unwrap();
        std::fs::write(
            source_fixture
                .store_root
                .join("runs")
                .join(&source_fixture.run_id.0)
                .join("artifacts/result.json"),
            b"tampered source bytes",
        )
        .unwrap();
        assert!(
            source_fixture
                .project_store
                .recover_actor_review_operation(&source_request, &source_grant)
                .is_err()
        );
        assert_operation_absent(&source_fixture, &source_request.operation_id);

        let authority_fixture = fixture();
        let authority_request =
            review_request(&authority_fixture, "op-review-recovery-authority-tamper");
        write_orphan_review(&authority_fixture, &authority_request);
        let authority_grant = ReviewRecoveryGrant::verify(
            &authority_fixture.project_store,
            &authority_fixture.science_store,
            &authority_request,
        )
        .unwrap();
        let authority_path = authority_fixture
            .store_root
            .join("runs")
            .join(&authority_fixture.authority_run_id.0)
            .join("run.json");
        let mut authority = authority_fixture
            .science_store
            .load_run(&authority_fixture.authority_run_id)
            .unwrap();
        authority
            .context
            .environment
            .remove(ReviewAdmission::ENV_REQUEST_SHA256);
        ProjectStore::write_json(&authority_path, &authority).unwrap();
        assert!(
            authority_fixture
                .project_store
                .recover_actor_review_operation(&authority_request, &authority_grant)
                .is_err()
        );
        assert_operation_absent(&authority_fixture, &authority_request.operation_id);

        let ledger_fixture = fixture();
        let ledger_request = review_request(&ledger_fixture, "op-review-recovery-ledger-tamper");
        let mut ledger_record = write_orphan_review(&ledger_fixture, &ledger_request);
        let ledger_grant = ReviewRecoveryGrant::verify(
            &ledger_fixture.project_store,
            &ledger_fixture.science_store,
            &ledger_request,
        )
        .unwrap();
        ledger_record.summary = "tampered review ledger".into();
        ProjectStore::write_json(
            &ledger_fixture
                .project_store
                .review_path(&ledger_fixture.project_id, &ledger_request.operation_id),
            &ledger_record,
        )
        .unwrap();
        assert!(
            ledger_fixture
                .project_store
                .recover_actor_review_operation(&ledger_request, &ledger_grant)
                .is_err()
        );
        assert_operation_absent(&ledger_fixture, &ledger_request.operation_id);
    }

    #[test]
    fn review_orphan_recovery_rejects_terminal_or_output_bearing_authority() {
        let terminal_fixture = fixture();
        let terminal_request =
            review_request(&terminal_fixture, "op-review-recovery-terminal-authority");
        write_orphan_review(&terminal_fixture, &terminal_request);
        let terminal_grant = ReviewRecoveryGrant::verify(
            &terminal_fixture.project_store,
            &terminal_fixture.science_store,
            &terminal_request,
        )
        .unwrap();
        terminal_fixture
            .science_store
            .transition(
                &terminal_fixture.authority_run_id,
                RunState::Failed,
                Some("interrupted before operation recovery".into()),
            )
            .unwrap();
        assert!(
            terminal_fixture
                .project_store
                .recover_actor_review_operation(&terminal_request, &terminal_grant)
                .is_err(),
            "a terminal authority filled a missing operation ledger"
        );
        assert_operation_absent(&terminal_fixture, &terminal_request.operation_id);

        let output_fixture = fixture();
        let output_request = review_request(&output_fixture, "op-review-recovery-output-authority");
        write_orphan_review(&output_fixture, &output_request);
        output_fixture
            .science_store
            .put_artifact(
                &ScienceProjectId::new(output_fixture.project_id.0.clone()),
                &output_fixture.authority_run_id,
                "owner-1",
                CallId::new("science_project_mutation"),
                Path::new("unexpected.json"),
                b"unexpected",
                "application/json",
                "unexpected pre-recovery output",
            )
            .unwrap();
        assert!(
            ReviewRecoveryGrant::verify(
                &output_fixture.project_store,
                &output_fixture.science_store,
                &output_request
            )
            .is_err(),
            "an output-bearing authority minted a recovery grant"
        );
        assert_operation_absent(&output_fixture, &output_request.operation_id);
    }

    #[test]
    fn review_orphan_recovery_rejects_legacy_v2_record() {
        let fixture = fixture();
        let request = review_request(&fixture, "op-review-recovery-legacy-v2");
        let mut record = write_orphan_review(&fixture, &request);
        record.schema_version = 2;
        ProjectStore::write_json(
            &fixture
                .project_store
                .review_path(&fixture.project_id, &request.operation_id),
            &record,
        )
        .unwrap();
        assert!(
            ReviewRecoveryGrant::verify(&fixture.project_store, &fixture.science_store, &request)
                .is_err(),
            "legacy v2 review minted a recovery grant"
        );
        assert_operation_absent(&fixture, &request.operation_id);
    }

    #[test]
    fn review_admission_is_stable_and_binds_actual_revision_without_caller_cas() {
        let fixture = fixture();
        let mut request = review_request(&fixture, "op-review-admission-stable");
        request.expected_revision = None;
        let first =
            ReviewAdmission::capture(&fixture.project_store, &fixture.science_store, &request)
                .unwrap();
        let second =
            ReviewAdmission::capture(&fixture.project_store, &fixture.science_store, &request)
                .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.sha256(), second.sha256());
        assert_eq!(
            first.project_revision(),
            fixture
                .project_store
                .project_revision(&fixture.project_id)
                .unwrap()
        );
        assert_eq!(
            first.request_sha256(),
            request.replay_fingerprint().unwrap()
        );
        assert_eq!(first.artifacts().len(), 1);
        assert_eq!(first.artifacts()[0].sha256, fixture.artifact_sha256);
    }

    #[test]
    fn legacy_v2_review_record_fails_closed_without_silent_upgrade() {
        let fixture = fixture();
        let request = review_request(&fixture, "op-review-legacy-v2");
        let outcome = apply_review(&fixture, &request).unwrap();
        let record: ReviewRecord = serde_json::from_value(outcome.result).unwrap();
        complete_authority_run(&fixture, &record);
        let path = fixture
            .project_store
            .review_path(&fixture.project_id, &request.operation_id);
        let mut legacy = record;
        legacy.schema_version = 2;
        ProjectStore::write_json(&path, &legacy).unwrap();

        assert!(
            fixture
                .project_store
                .list_reviews_with_store(&fixture.science_store, &fixture.project_id)
                .is_err(),
            "legacy v2 review was silently admitted"
        );
        let durable: ReviewRecord = ProjectStore::read_json(&path).unwrap();
        assert_eq!(durable.schema_version, 2, "legacy record was rewritten");
    }

    #[test]
    fn review_finish_rejects_source_authority_replacement_after_admission_without_writes() {
        let fixture = fixture();
        let request = review_request(&fixture, "op-review-admission-source-race");
        let admission = admit_review(&fixture, &request).unwrap();
        let evidence_path = fixture
            .store_root
            .join("runs")
            .join(&fixture.run_id.0)
            .join("evidence.json");
        let mut evidence: Vec<crate::Evidence> = ProjectStore::read_json(&evidence_path).unwrap();
        evidence[0].claim = "Changed after Allow, but still structurally valid.".into();
        evidence[0].source = "fixture://changed-after-allow".into();
        ProjectStore::write_json(&evidence_path, &evidence).unwrap();

        let error = fixture
            .project_store
            .apply_actor_review(&request, &fixture.science_store, &admission)
            .unwrap_err();
        assert!(
            error.to_string().contains("changed after admission"),
            "source replacement failed for the wrong reason: {error}"
        );
        assert_no_review_commit(&fixture, &request.operation_id);
    }

    #[test]
    fn review_finish_rejects_project_revision_race_without_writes() {
        let fixture = fixture();
        let mut request = review_request(&fixture, "op-review-admission-project-race");
        request.expected_revision = None;
        let admission = admit_review(&fixture, &request).unwrap();
        fixture
            .project_store
            .apply_mutation(&MutationRequest {
                operation_id: "op-review-race-question-update".into(),
                session_id: "session-1".into(),
                owner_id: "owner-1".into(),
                expected_revision: Some(
                    fixture
                        .project_store
                        .project_revision(&fixture.project_id)
                        .unwrap(),
                ),
                mutation: ProjectMutation::QuestionUpdate {
                    project_id: fixture.project_id.clone(),
                    research_question: "Did the question move after Allow?".into(),
                },
            })
            .unwrap();

        let error = fixture
            .project_store
            .apply_actor_review(&request, &fixture.science_store, &admission)
            .unwrap_err();
        assert!(
            error.to_string().contains("changed after admission"),
            "project revision race failed for the wrong reason: {error}"
        );
        assert_no_review_commit(&fixture, &request.operation_id);
    }

    #[test]
    fn review_finish_rejects_request_admission_mismatch_without_writes() {
        let fixture = fixture();
        let request = review_request(&fixture, "op-review-admission-request-race");
        let admission = admit_review(&fixture, &request).unwrap();
        let mut changed = request.clone();
        if let ProjectMutation::ReviewRecord { summary, .. } = &mut changed.mutation {
            *summary = "A different, still valid summary submitted after Allow.".into();
        }

        let error = fixture
            .project_store
            .apply_actor_review(&changed, &fixture.science_store, &admission)
            .unwrap_err();
        assert!(
            error.to_string().contains("changed after admission"),
            "request mismatch failed for the wrong reason: {error}"
        );
        assert_no_review_commit(&fixture, &request.operation_id);
    }

    #[test]
    fn review_finish_requires_exact_authority_environment_without_writes() {
        let fixture = fixture();
        let request = review_request(&fixture, "op-review-admission-env");
        let admission = admit_review(&fixture, &request).unwrap();
        let authority_path = fixture
            .store_root
            .join("runs")
            .join(&fixture.authority_run_id.0)
            .join("run.json");
        let mut authority = fixture
            .science_store
            .load_run(&fixture.authority_run_id)
            .unwrap();
        authority
            .context
            .environment
            .remove(ReviewAdmission::ENV_SOURCE_AUTHORITY_SHA256);
        ProjectStore::write_json(&authority_path, &authority).unwrap();

        let error = fixture
            .project_store
            .apply_actor_review(&request, &fixture.science_store, &admission)
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("exact review_source_authority_sha256"),
            "authority environment mismatch failed for the wrong reason: {error}"
        );
        assert_no_review_commit(&fixture, &request.operation_id);
    }

    #[test]
    fn review_cannot_succeed_before_preterminal_commit_verification() {
        let fixture = fixture();
        let outcome = apply_review(
            &fixture,
            &review_request(&fixture, "op-review-preterminal-tamper"),
        )
        .unwrap();
        let record: ReviewRecord = serde_json::from_value(outcome.result).unwrap();
        let science_store = &fixture.science_store;
        let manifest = serde_json::to_vec_pretty(&record).unwrap();
        let artifact = science_store
            .put_artifact(
                &ScienceProjectId::new(fixture.project_id.0.clone()),
                &fixture.authority_run_id,
                "owner-1",
                CallId::new("science_project_mutation"),
                Path::new("review_record.json"),
                &manifest,
                "application/json",
                "actor-owned durable review record",
            )
            .unwrap();
        science_store
            .add_evidence(record.expected_evidence(artifact.sha256))
            .unwrap();
        science_store
            .add_provenance(record.expected_provenance())
            .unwrap();

        let manifest_path = fixture
            .store_root
            .join("runs")
            .join(&fixture.authority_run_id.0)
            .join("artifacts/review_record.json");
        std::fs::write(&manifest_path, br#"{"forged":true}"#).unwrap();

        assert!(
            fixture
                .project_store
                .verify_pending_review_commit_with_store(science_store, &record)
                .is_err(),
            "tampered review commit passed its preterminal authority check"
        );
        assert_eq!(
            science_store
                .load_run(&fixture.authority_run_id)
                .unwrap()
                .state,
            RunState::Running,
            "failed preterminal verification must not expose Succeeded"
        );
        assert!(
            fixture
                .project_store
                .list_reviews_with_store(&fixture.science_store, &fixture.project_id)
                .is_err(),
            "a running authority commit must not expose a durable review"
        );
    }

    #[test]
    fn review_rehashes_persists_moves_revision_and_replays_once() {
        let fixture = fixture();
        let before = fixture
            .project_store
            .project_revision(&fixture.project_id)
            .unwrap();
        let request = review_request(&fixture, "op-review-0001");
        let mut mismatched_request = request.clone();
        if let ProjectMutation::ReviewRecord { summary, .. } = &mut mismatched_request.mutation {
            *summary =
                "A different replay admission must not authorize the original request.".into();
        }
        let mismatched_admission = ReviewAdmission::capture(
            &fixture.project_store,
            &fixture.science_store,
            &mismatched_request,
        )
        .unwrap();
        let admission = admit_review(&fixture, &request).unwrap();
        let first = fixture
            .project_store
            .apply_actor_review(&request, &fixture.science_store, &admission)
            .unwrap();
        assert_eq!(first.kind, "review_record");
        assert_ne!(first.revision, before);
        let record: ReviewRecord = serde_json::from_value(first.result.clone()).unwrap();
        assert_eq!(record.review_id, "op-review-0001");
        assert_eq!(record.source_run_id, "source-run-1");
        assert_eq!(record.artifacts.len(), 1);
        assert_eq!(record.artifacts[0].sha256, fixture.artifact_sha256);
        assert_eq!(record.evidence_fingerprint.len(), 64);

        // A crash after the project-ledger write but before the actor commits
        // its manifest/evidence/provenance must never expose a valid review or
        // a replayable success.
        assert!(
            fixture
                .project_store
                .list_reviews_with_store(&fixture.science_store, &fixture.project_id)
                .is_err()
        );
        assert!(
            fixture
                .project_store
                .apply_actor_review(&request, &fixture.science_store, &admission)
                .is_err()
        );

        complete_authority_run(&fixture, &record);

        let reopened = ProjectStore::new(&fixture.store_root);
        let reopened_science = ScienceStore::new(&fixture.store_root);
        let records = reopened
            .list_reviews_with_store(&reopened_science, &fixture.project_id)
            .unwrap();
        assert_eq!(records, vec![record.clone()]);

        assert!(
            reopened
                .apply_actor_review(&request, &reopened_science, &mismatched_admission)
                .is_err(),
            "a different immutable admission replayed the original review"
        );
        assert_eq!(
            reopened
                .list_reviews_with_store(&reopened_science, &fixture.project_id)
                .unwrap()
                .len(),
            1,
            "rejected admission replay changed the review ledger"
        );
        let replay = reopened
            .apply_actor_review(&request, &reopened_science, &admission)
            .unwrap();
        assert!(replay.replayed);
        assert_eq!(
            reopened
                .list_reviews_with_store(&reopened_science, &fixture.project_id)
                .unwrap()
                .len(),
            1
        );

        let authority_dir = fixture
            .store_root
            .join("runs")
            .join(&fixture.authority_run_id.0);
        let approvals_path = authority_dir.join("approvals.json");
        let approvals: Vec<crate::Approval> = ProjectStore::read_json(&approvals_path).unwrap();
        let mut wrong_approvals = approvals.clone();
        wrong_approvals[0].call_id = CallId::new("forged-review-call");
        ProjectStore::write_json(&approvals_path, &wrong_approvals).unwrap();
        assert!(
            reopened
                .list_reviews_with_store(&reopened_science, &fixture.project_id)
                .is_err(),
            "review approval call-id tamper was accepted"
        );
        ProjectStore::write_json(&approvals_path, &approvals).unwrap();

        let artifacts_path = authority_dir.join("artifacts.json");
        let artifacts: Vec<crate::Artifact> = ProjectStore::read_json(&artifacts_path).unwrap();
        for (field, mutate) in [
            ("call_id", |artifact: &mut crate::Artifact| {
                artifact.call_id = CallId::new("forged-review-call")
            }),
            ("mime", |artifact: &mut crate::Artifact| {
                artifact.mime = "text/plain".into()
            }),
            ("preview", |artifact: &mut crate::Artifact| {
                artifact.preview = "forged preview".into()
            }),
        ] as [(&str, fn(&mut crate::Artifact)); 3]
        {
            let mut tampered = artifacts.clone();
            mutate(&mut tampered[0]);
            ProjectStore::write_json(&artifacts_path, &tampered).unwrap();
            assert!(
                reopened
                    .list_reviews_with_store(&reopened_science, &fixture.project_id)
                    .is_err(),
                "review manifest {field} tamper was accepted"
            );
            ProjectStore::write_json(&artifacts_path, &artifacts).unwrap();
        }

        let provenance_path = fixture
            .store_root
            .join("runs")
            .join(&fixture.authority_run_id.0)
            .join("provenance.json");
        let mut wrong_provenance = record.expected_provenance();
        wrong_provenance.input_sha256 = "f".repeat(64);
        ProjectStore::write_json(&provenance_path, &vec![wrong_provenance]).unwrap();
        assert!(
            reopened
                .list_reviews_with_store(&reopened_science, &fixture.project_id)
                .is_err(),
            "review-specific provenance tamper was accepted"
        );
        ProjectStore::write_json(&provenance_path, &vec![record.expected_provenance()]).unwrap();
        assert_eq!(
            reopened
                .list_reviews_with_store(&reopened_science, &fixture.project_id)
                .unwrap()
                .len(),
            1
        );

        // A bare filesystem write cannot mint a second authoritative review:
        // its claimed authority run does not exist and list therefore fails
        // closed instead of returning the forged JSON.
        let mut forged = record;
        forged.review_id = "op-review-forged".into();
        forged.operation_id = "op-review-forged".into();
        forged.authority_run_id = "forged-authority-run".into();
        let forged_path = reopened.review_path(&fixture.project_id, "op-review-forged");
        ProjectStore::write_json(&forged_path, &forged).unwrap();
        assert!(
            reopened
                .list_reviews_with_store(&reopened_science, &fixture.project_id)
                .is_err()
        );
    }

    #[test]
    fn bare_project_mutation_cannot_bypass_actor_review_authority() {
        let fixture = fixture();
        let request = review_request(&fixture, "op-review-bare-bypass");
        let error = fixture.project_store.apply_mutation(&request).unwrap_err();
        assert!(
            matches!(error, ScienceError::Invalid(ref message) if message.contains("SessionActor-retained")),
            "bare review failed for the wrong reason: {error}"
        );
        assert!(
            fixture
                .project_store
                .lookup_operation("op-review-bare-bypass")
                .unwrap()
                .is_none()
        );
    }

    #[cfg(unix)]
    #[test]
    fn review_finish_uses_retained_root_after_pathname_rename_and_replace() {
        let fixture = fixture();
        let request = review_request(&fixture, "op-review-retained-root");
        let admission = admit_review(&fixture, &request).unwrap();
        let retained_root = fixture._root.path().join("retained-science-store");
        std::fs::rename(&fixture.store_root, &retained_root).unwrap();
        std::fs::create_dir(&fixture.store_root).unwrap();

        let outcome = fixture
            .project_store
            .apply_actor_review(&request, &fixture.science_store, &admission)
            .unwrap();
        let record: ReviewRecord = serde_json::from_value(outcome.result).unwrap();
        complete_authority_run(&fixture, &record);
        fixture
            .project_store
            .verify_review_record_with_store(&fixture.science_store, &record)
            .unwrap();

        assert!(
            retained_root
                .join("projects")
                .join(&fixture.project_id.0)
                .join("reviews")
                .join("op-review-retained-root.json")
                .is_file(),
            "review was not written through the retained project capability"
        );
        assert_eq!(
            std::fs::read_dir(&fixture.store_root).unwrap().count(),
            0,
            "replacement pathname received review or authority bytes"
        );
    }

    #[test]
    fn review_source_requires_exact_approval_evidence_and_provenance() {
        let approval_fixture = fixture();
        let source_dir = approval_fixture
            .store_root
            .join("runs")
            .join(&approval_fixture.run_id.0);
        let approval_path = source_dir.join("approvals.json");
        let mut approvals: Vec<crate::Approval> = ProjectStore::read_json(&approval_path).unwrap();
        approvals[0].call_id = CallId::new("forged-source-call");
        ProjectStore::write_json(&approval_path, &approvals).unwrap();
        assert!(
            apply_review(
                &approval_fixture,
                &review_request(&approval_fixture, "op-review-source-approval")
            )
            .is_err(),
            "source approval call-id tamper was accepted"
        );

        let evidence_fixture = fixture();
        let source_dir = evidence_fixture
            .store_root
            .join("runs")
            .join(&evidence_fixture.run_id.0);
        let evidence_path = source_dir.join("evidence.json");
        let mut evidence: Vec<crate::Evidence> = ProjectStore::read_json(&evidence_path).unwrap();
        evidence[0].artifact_sha256 = None;
        ProjectStore::write_json(&evidence_path, &evidence).unwrap();
        assert!(
            apply_review(
                &evidence_fixture,
                &review_request(&evidence_fixture, "op-review-source-evidence")
            )
            .is_err(),
            "source evidence registry tamper was accepted"
        );

        let provenance_fixture = fixture();
        let source_dir = provenance_fixture
            .store_root
            .join("runs")
            .join(&provenance_fixture.run_id.0);
        let provenance_path = source_dir.join("provenance.json");
        let mut provenance: Vec<crate::Provenance> =
            ProjectStore::read_json(&provenance_path).unwrap();
        provenance[0].run_id = RunId::new("forged-source-run");
        ProjectStore::write_json(&provenance_path, &provenance).unwrap();
        assert!(
            apply_review(
                &provenance_fixture,
                &review_request(&provenance_fixture, "op-review-source-provenance")
            )
            .is_err(),
            "source provenance run binding tamper was accepted"
        );
    }

    #[test]
    fn review_replay_rejects_format_valid_source_authority_replacement() {
        let evidence_fixture = fixture();
        let evidence_request =
            review_request(&evidence_fixture, "op-review-source-evidence-replacement");
        let evidence_admission = admit_review(&evidence_fixture, &evidence_request).unwrap();
        let outcome = evidence_fixture
            .project_store
            .apply_actor_review(
                &evidence_request,
                &evidence_fixture.science_store,
                &evidence_admission,
            )
            .unwrap();
        let evidence_record: ReviewRecord = serde_json::from_value(outcome.result).unwrap();
        complete_authority_run(&evidence_fixture, &evidence_record);
        let source_dir = evidence_fixture
            .store_root
            .join("runs")
            .join(&evidence_fixture.run_id.0);
        let evidence_path = source_dir.join("evidence.json");
        let mut evidence: Vec<crate::Evidence> = ProjectStore::read_json(&evidence_path).unwrap();
        evidence[0].claim = "A different but still non-empty claim.".into();
        evidence[0].source = "fixture://replacement-evidence".into();
        ProjectStore::write_json(&evidence_path, &evidence).unwrap();
        assert!(
            evidence_fixture
                .project_store
                .verify_review_record_with_store(&evidence_fixture.science_store, &evidence_record,)
                .is_err(),
            "format-valid source evidence replacement retained an old review proof"
        );
        assert!(
            evidence_fixture
                .project_store
                .apply_actor_review(
                    &evidence_request,
                    &evidence_fixture.science_store,
                    &evidence_admission,
                )
                .is_err(),
            "format-valid source evidence replacement replayed an old admission"
        );

        let provenance_fixture = fixture();
        let provenance_request = review_request(
            &provenance_fixture,
            "op-review-source-provenance-replacement",
        );
        let provenance_admission = admit_review(&provenance_fixture, &provenance_request).unwrap();
        let outcome = provenance_fixture
            .project_store
            .apply_actor_review(
                &provenance_request,
                &provenance_fixture.science_store,
                &provenance_admission,
            )
            .unwrap();
        let provenance_record: ReviewRecord = serde_json::from_value(outcome.result).unwrap();
        complete_authority_run(&provenance_fixture, &provenance_record);
        let source_dir = provenance_fixture
            .store_root
            .join("runs")
            .join(&provenance_fixture.run_id.0);
        let provenance_path = source_dir.join("provenance.json");
        let mut provenance: Vec<crate::Provenance> =
            ProjectStore::read_json(&provenance_path).unwrap();
        provenance[0].source_uri = "fixture://replacement-provenance".into();
        provenance[0].tool = "replacement-but-valid-tool".into();
        ProjectStore::write_json(&provenance_path, &provenance).unwrap();
        assert!(
            provenance_fixture
                .project_store
                .verify_review_record_with_store(
                    &provenance_fixture.science_store,
                    &provenance_record,
                )
                .is_err(),
            "format-valid source provenance replacement retained an old review proof"
        );
        assert!(
            provenance_fixture
                .project_store
                .apply_actor_review(
                    &provenance_request,
                    &provenance_fixture.science_store,
                    &provenance_admission,
                )
                .is_err(),
            "format-valid source provenance replacement replayed an old admission"
        );
    }

    #[test]
    fn review_source_missing_artifact_registry_fails_before_project_write() {
        let fixture = fixture();
        let registry = fixture
            .store_root
            .join("runs")
            .join(&fixture.run_id.0)
            .join("artifacts.json");
        ProjectStore::write_json(&registry, &Vec::<crate::Artifact>::new()).unwrap();
        assert!(
            apply_review(
                &fixture,
                &review_request(&fixture, "op-review-missing-source-registry")
            )
            .is_err()
        );
        assert!(
            fixture
                .project_store
                .lookup_operation("op-review-missing-source-registry")
                .unwrap()
                .is_none(),
            "missing source registry still committed a project operation"
        );
    }

    #[test]
    fn review_fails_closed_on_tamper_unknown_hash_or_identity_mismatch() {
        let tampered = fixture();
        let artifact_path = tampered
            .store_root
            .join("runs")
            .join(&tampered.run_id.0)
            .join("artifacts/result.json");
        std::fs::write(&artifact_path, b"tampered").unwrap();
        assert!(
            apply_review(&tampered, &review_request(&tampered, "op-review-tamper")).is_err(),
            "tampered bytes produced a review"
        );
        assert!(
            tampered
                .project_store
                .list_reviews_with_store(&tampered.science_store, &tampered.project_id)
                .unwrap()
                .is_empty()
        );
        assert!(
            tampered
                .project_store
                .lookup_operation("op-review-tamper")
                .unwrap()
                .is_none()
        );

        let clean = fixture();
        let mut unknown = review_request(&clean, "op-review-unknown");
        if let ProjectMutation::ReviewRecord {
            artifact_sha256s, ..
        } = &mut unknown.mutation
        {
            *artifact_sha256s = vec!["a".repeat(64)];
        }
        assert!(apply_review(&clean, &unknown).is_err());

        let mut wrong_session = review_request(&clean, "op-review-session");
        wrong_session.session_id = "session-2".into();
        assert!(matches!(
            apply_review(&clean, &wrong_session),
            Err(ScienceError::Ownership)
        ));

        let mut forged_reviewer = review_request(&clean, "op-review-reviewer");
        if let ProjectMutation::ReviewRecord { reviewer_id, .. } = &mut forged_reviewer.mutation {
            *reviewer_id = "Nature-Reviewer-2".into();
        }
        assert!(matches!(
            apply_review(&clean, &forged_reviewer),
            Err(ScienceError::Ownership)
        ));

        let other = clean
            .project_store
            .create_project("owner-1", "Other project", "Wrong boundary?")
            .unwrap();
        let mut wrong_project = review_request(&clean, "op-review-project");
        if let ProjectMutation::ReviewRecord { project_id, .. } = &mut wrong_project.mutation {
            *project_id = other.project_id;
        }
        wrong_project.expected_revision = None;
        assert!(matches!(
            apply_review(&clean, &wrong_project),
            Err(ScienceError::Ownership)
        ));

        let mut traversal = review_request(&clean, "op-review-traversal");
        if let ProjectMutation::ReviewRecord { project_id, .. } = &mut traversal.mutation {
            *project_id = ProjectId("../operations".into());
        }
        traversal.expected_revision = None;
        let error = apply_review(&clean, &traversal).unwrap_err();
        assert!(
            matches!(&error, ScienceError::Invalid(message) if message.contains("projectId")),
            "project-id traversal failed for the wrong reason: {error}"
        );

        #[cfg(unix)]
        {
            let linked = fixture();
            let linked_request = review_request(&linked, "op-review-symlink");
            let outcome = apply_review(&linked, &linked_request).unwrap();
            let record: ReviewRecord = serde_json::from_value(outcome.result).unwrap();
            complete_authority_run(&linked, &record);
            let reviews_dir = linked.project_store.reviews_dir(&linked.project_id);
            std::os::unix::fs::symlink(
                reviews_dir.join("op-review-symlink.json"),
                reviews_dir.join("linked-review.json"),
            )
            .unwrap();
            assert!(
                linked
                    .project_store
                    .list_reviews_with_store(&linked.science_store, &linked.project_id)
                    .is_err(),
                "a symlinked review ledger entry was followed"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn review_ledger_parent_symlink_cannot_redirect_read_or_write() {
        use std::os::unix::fs::symlink;

        let write_fixture = fixture();
        let request = review_request(&write_fixture, "op-review-parent-symlink");
        let outside = write_fixture._root.path().join("outside-reviews");
        std::fs::create_dir_all(&outside).unwrap();
        symlink(
            &outside,
            write_fixture
                .project_store
                .reviews_dir(&write_fixture.project_id),
        )
        .unwrap();

        assert!(
            apply_review(&write_fixture, &request).is_err(),
            "a symlinked review parent accepted a ledger write"
        );
        assert_eq!(
            std::fs::read_dir(&outside).unwrap().count(),
            0,
            "the rejected review write created bytes outside the project store"
        );
        assert!(
            write_fixture
                .project_store
                .lookup_operation("op-review-parent-symlink")
                .unwrap()
                .is_none(),
            "a rejected review write still committed its operation ledger"
        );

        let read_fixture = fixture();
        let outcome = apply_review(
            &read_fixture,
            &review_request(&read_fixture, "op-review-parent-read"),
        )
        .unwrap();
        let record: ReviewRecord = serde_json::from_value(outcome.result).unwrap();
        complete_authority_run(&read_fixture, &record);
        let reviews = read_fixture
            .project_store
            .reviews_dir(&read_fixture.project_id);
        let retained = read_fixture._root.path().join("retained-reviews");
        let forged = read_fixture._root.path().join("forged-reviews");
        std::fs::rename(&reviews, &retained).unwrap();
        std::fs::create_dir_all(&forged).unwrap();
        ProjectStore::write_json(&forged.join("forged.json"), &record).unwrap();
        symlink(&forged, &reviews).unwrap();

        assert!(
            read_fixture
                .project_store
                .list_reviews_with_store(&read_fixture.science_store, &read_fixture.project_id)
                .is_err(),
            "review listing followed a replaced parent-directory symlink"
        );
    }
}
