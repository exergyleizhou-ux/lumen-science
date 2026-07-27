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
            input_sha256: self.evidence_fingerprint.clone(),
            tool: "SessionActor/review_record-v1".into(),
            environment: BTreeMap::from([
                ("source_run_id".into(), self.source_run_id.clone()),
                ("artifact_count".into(), self.artifacts.len().to_string()),
                ("network".into(), "disabled".into()),
            ]),
        }
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

    fn reviews_dir(&self, project_id: &ProjectId) -> PathBuf {
        self.project_dir(project_id).join("reviews")
    }

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

    pub fn list_reviews(&self, project_id: &ProjectId) -> crate::Result<Vec<ReviewRecord>> {
        self.gates().require(ScienceFeature::Collaboration)?;
        self.gates().require(ScienceFeature::ReviewPackage)?;
        let project = self.load_project(project_id)?;
        let dir = self.reviews_dir(project_id);
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut records = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            if !entry.file_type()?.is_file() {
                return Err(ScienceError::Invalid(
                    "review ledger contains a non-regular JSON entry".into(),
                ));
            }
            let record: ReviewRecord = Self::read_json(&path)?;
            if record.project_id != *project_id || record.owner_id != project.owner_id.0 {
                return Err(ScienceError::Ownership);
            }
            self.verify_review_record(&record)?;
            records.push(record);
        }
        records.sort_by(|left, right| {
            left.recorded_at
                .cmp(&right.recorded_at)
                .then_with(|| left.review_id.cmp(&right.review_id))
        });
        Ok(records)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_review_inner(
        &self,
        operation_id: &str,
        session_id: &str,
        owner_id: &str,
        project_id: &ProjectId,
        reviewer_id: &str,
        verdict: ReviewVerdict,
        summary: &str,
        claim_id: Option<String>,
        source_run_id: &str,
        authority_run_id: &str,
        artifact_sha256s: &[String],
    ) -> crate::Result<ReviewRecord> {
        self.gates().require(ScienceFeature::EvidenceGraph)?;
        self.gates().require(ScienceFeature::Collaboration)?;
        self.gates().require(ScienceFeature::ReviewPackage)?;
        if reviewer_id.trim().is_empty() || reviewer_id.len() > 128 {
            return Err(ScienceError::Invalid(
                "reviewerId must be 1..=128 characters".into(),
            ));
        }
        if reviewer_id != owner_id {
            return Err(ScienceError::Ownership);
        }
        if summary.trim().is_empty() || summary.len() > 16_384 {
            return Err(ScienceError::Invalid(
                "review summary must be 1..=16384 characters".into(),
            ));
        }
        Self::validate_run_id(source_run_id, "runId")?;
        Self::validate_run_id(authority_run_id, "authority run id")?;
        if artifact_sha256s.is_empty() || artifact_sha256s.len() > 128 {
            return Err(ScienceError::Invalid(
                "review requires 1..=128 artifact SHA-256 values".into(),
            ));
        }

        let project = self.load_project(project_id)?;
        if project.owner_id.0 != owner_id {
            return Err(ScienceError::Ownership);
        }
        if let Some(claim_id) = claim_id.as_deref() {
            let claim = self.load_claim(project_id, claim_id)?;
            if claim.project_id != *project_id {
                return Err(ScienceError::Ownership);
            }
        }

        let mut requested = BTreeSet::new();
        for sha256 in artifact_sha256s {
            validate_sha256_hex(sha256).map_err(ScienceError::Invalid)?;
            if !requested.insert(sha256.clone()) {
                return Err(ScienceError::Invalid(
                    "review artifact SHA-256 values must be unique".into(),
                ));
            }
        }

        // This store root is deliberately shared by ProjectStore and
        // ScienceStore. The actor validates that root against its workspace
        // before approval; the checks below bind the cited run back to the
        // same owner/project/session/workspace and exact run root.
        let science_store = ScienceStore::new(self.root());
        let run_id = RunId::new(source_run_id);
        let run = science_store.load_run(&run_id)?;
        let science_project = crate::ProjectId::new(project_id.0.clone());
        if run.state != RunState::Succeeded {
            return Err(ScienceError::Invalid(
                "review artifacts must come from a succeeded run".into(),
            ));
        }
        if run.context.project_id != science_project
            || run.context.owner_id != owner_id
            || run.context.session_id != session_id
        {
            return Err(ScienceError::Ownership);
        }

        let authority_id = RunId::new(authority_run_id);
        let authority = science_store.load_run(&authority_id)?;
        if authority.state != RunState::Running
            || authority.context.project_id != science_project
            || authority.context.owner_id != owner_id
            || authority.context.session_id != session_id
            || authority.context.workspace_root != run.context.workspace_root
            || authority.context.artifact_root != run.context.artifact_root
        {
            return Err(ScienceError::Ownership);
        }
        let approvals = science_store.approvals(&authority_id)?;
        if approvals.is_empty()
            || approvals
                .iter()
                .any(|approval| approval.decision != crate::ApprovalDecision::Allow)
        {
            return Err(ScienceError::Invalid(
                "review authority run requires terminal Allow approval".into(),
            ));
        }
        let canonical_store = dunce::canonicalize(self.root())?;
        let canonical_workspace = dunce::canonicalize(&run.context.workspace_root)?;
        let canonical_run_root = dunce::canonicalize(&run.context.artifact_root)?;
        if !canonical_store.starts_with(&canonical_workspace)
            || canonical_run_root != canonical_store.join("runs")
        {
            return Err(ScienceError::Invalid(
                "review source run is outside its bound workspace/store root".into(),
            ));
        }

        let mut registered = science_store.artifacts(&run_id)?;
        registered.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
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
                        "artifact {sha256} is not registered in source run {source_run_id}"
                    )));
                }
                _ => {
                    return Err(ScienceError::Invalid(format!(
                        "artifact {sha256} is ambiguous in source run {source_run_id}"
                    )));
                }
            };
            let bytes = science_store.artifact_bytes(
                &science_project,
                &run_id,
                owner_id,
                &artifact.relative_path,
            )?;
            if bytes.len() as u64 != artifact.bytes || crate::hex_sha256(&bytes) != artifact.sha256
            {
                return Err(ScienceError::Invalid(
                    "review artifact bytes do not match their registered hash/length".into(),
                ));
            }
            reviewed.push(ReviewedArtifact {
                source_run_id: source_run_id.to_string(),
                relative_path: artifact.relative_path.clone(),
                sha256: artifact.sha256.clone(),
                bytes: artifact.bytes,
                mime: artifact.mime.clone(),
            });
        }

        let evidence_fingerprint = Self::review_fingerprint(project_id, source_run_id, &reviewed);
        let record = ReviewRecord {
            schema_version: 1,
            review_id: operation_id.to_string(),
            operation_id: operation_id.to_string(),
            project_id: project_id.clone(),
            owner_id: owner_id.to_string(),
            session_id: session_id.to_string(),
            reviewer_id: reviewer_id.to_string(),
            verdict,
            summary: summary.to_string(),
            claim_id,
            source_run_id: source_run_id.to_string(),
            authority_run_id: authority_run_id.to_string(),
            artifacts: reviewed,
            evidence_fingerprint,
            recorded_at: Utc::now(),
        };

        let path = self.review_path(project_id, operation_id);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if !metadata.file_type().is_file() {
                    return Err(ScienceError::Invalid(
                        "review operation path is not a regular file".into(),
                    ));
                }
                let existing: ReviewRecord = Self::read_json(&path)?;
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
                    && existing.evidence_fingerprint == record.evidence_fingerprint
                {
                    return Ok(existing);
                }
                return Err(ScienceError::Invalid(format!(
                    "review operation {operation_id} already exists with different content"
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Self::write_json(&path, &record)?;
        Ok(record)
    }

    fn verify_review_record_binding(
        &self,
        record: &ReviewRecord,
        authority_state: RunState,
    ) -> crate::Result<(ScienceStore, RunId)> {
        super::mutation::validate_operation_id(&record.operation_id)?;
        Self::validate_run_id(&record.source_run_id, "source run id")?;
        Self::validate_run_id(&record.authority_run_id, "authority run id")?;
        if record.schema_version != 1
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
        validate_sha256_hex(&record.evidence_fingerprint).map_err(ScienceError::Invalid)?;
        let project = self.load_project(&record.project_id)?;
        if project.owner_id.0 != record.owner_id || record.reviewer_id != record.owner_id {
            return Err(ScienceError::Ownership);
        }
        let ledger_path = self.review_path(&record.project_id, &record.operation_id);
        let ledger_metadata = std::fs::symlink_metadata(&ledger_path)?;
        if !ledger_metadata.file_type().is_file() {
            return Err(ScienceError::Invalid(
                "project review ledger entry is not a regular file".into(),
            ));
        }
        let ledger_record: ReviewRecord = Self::read_json(&ledger_path)?;
        if ledger_record != *record {
            return Err(ScienceError::Invalid(
                "project review ledger does not match its operation result".into(),
            ));
        }
        let science_store = ScienceStore::new(self.root());
        let source_run = RunId::new(&record.source_run_id);
        let source = science_store.load_run(&source_run)?;
        if source.state != RunState::Succeeded
            || source.context.project_id.0 != record.project_id.0
            || source.context.owner_id != record.owner_id
            || source.context.session_id != record.session_id
        {
            return Err(ScienceError::Ownership);
        }
        let canonical_store = dunce::canonicalize(self.root())?;
        let canonical_workspace = dunce::canonicalize(&source.context.workspace_root)?;
        let canonical_source_root = dunce::canonicalize(&source.context.artifact_root)?;
        if !canonical_store.starts_with(&canonical_workspace)
            || canonical_source_root != canonical_store.join("runs")
        {
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
        let canonical_authority_workspace = dunce::canonicalize(&authority.context.workspace_root)?;
        let canonical_authority_root = dunce::canonicalize(&authority.context.artifact_root)?;
        if canonical_authority_workspace != canonical_workspace
            || canonical_authority_root != canonical_source_root
        {
            return Err(ScienceError::Invalid(
                "review authority run is outside its source-run workspace/store".into(),
            ));
        }
        let approvals = science_store.approvals(&authority_run)?;
        if approvals.is_empty()
            || approvals
                .iter()
                .any(|approval| approval.decision != crate::ApprovalDecision::Allow)
        {
            return Err(ScienceError::Invalid(
                "review authority run has no terminal Allow approval".into(),
            ));
        }
        Ok((science_store, authority_run))
    }

    /// Validate the durable project + operation half of a review commit while
    /// its already-Allowed authority run is still Running. The SessionActor
    /// uses this only to recover an interrupted evidence commit; callers
    /// cannot turn a denied/failed run back into authority.
    pub fn verify_pending_review_record(&self, record: &ReviewRecord) -> crate::Result<()> {
        self.verify_review_record_binding(record, RunState::Running)?;
        Ok(())
    }

    /// Fail closed unless the review's own authority run is complete and its
    /// manifest/evidence/provenance exactly bind the project-ledger record.
    pub fn verify_review_record(&self, record: &ReviewRecord) -> crate::Result<()> {
        let (science_store, authority_run) =
            self.verify_review_record_binding(record, RunState::Succeeded)?;
        crate::review::verify_for_goal_completion(&science_store, &authority_run)?;
        let artifacts = science_store.artifacts(&authority_run)?;
        let [manifest] = artifacts.as_slice() else {
            return Err(ScienceError::Invalid(
                "review authority run must contain exactly one manifest artifact".into(),
            ));
        };
        if manifest.relative_path != std::path::Path::new("review_record.json") {
            return Err(ScienceError::Invalid(
                "review authority run has no canonical review manifest".into(),
            ));
        }
        let manifest_bytes = science_store.artifact_bytes(
            &crate::ProjectId::new(record.project_id.0.clone()),
            &authority_run,
            &record.owner_id,
            &manifest.relative_path,
        )?;
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
        let artifact = science_store
            .put_artifact(
                &ScienceProjectId::new(project.project_id.0.clone()),
                &run_id,
                "owner-1",
                CallId::new("source-call"),
                Path::new("result.json"),
                br#"{"result":"verified"}"#,
                "application/json",
                "source result",
            )
            .unwrap();
        science_store
            .transition(&run_id, RunState::Succeeded, None)
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
        let authority_call = CallId::new("review-authority-call");
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

    fn complete_authority_run(fixture: &Fixture, record: &ReviewRecord) {
        let science_store = ScienceStore::new(&fixture.store_root);
        let manifest = serde_json::to_vec_pretty(record).unwrap();
        let artifact = science_store
            .put_artifact(
                &ScienceProjectId::new(fixture.project_id.0.clone()),
                &fixture.authority_run_id,
                "owner-1",
                CallId::new("review-authority-call"),
                Path::new("review_record.json"),
                &manifest,
                "application/json",
                "review manifest",
            )
            .unwrap();
        science_store
            .add_evidence(record.expected_evidence(artifact.sha256))
            .unwrap();
        science_store
            .add_provenance(record.expected_provenance())
            .unwrap();
        science_store
            .transition(&fixture.authority_run_id, RunState::Succeeded, None)
            .unwrap();
    }

    #[test]
    fn review_rehashes_persists_moves_revision_and_replays_once() {
        let fixture = fixture();
        let before = fixture
            .project_store
            .project_revision(&fixture.project_id)
            .unwrap();
        let request = review_request(&fixture, "op-review-0001");
        let first = fixture.project_store.apply_mutation(&request).unwrap();
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
                .list_reviews(&fixture.project_id)
                .is_err()
        );
        assert!(fixture.project_store.apply_mutation(&request).is_err());

        complete_authority_run(&fixture, &record);

        let reopened = ProjectStore::new(&fixture.store_root);
        let records = reopened.list_reviews(&fixture.project_id).unwrap();
        assert_eq!(records, vec![record.clone()]);

        let replay = reopened.apply_mutation(&request).unwrap();
        assert!(replay.replayed);
        assert_eq!(reopened.list_reviews(&fixture.project_id).unwrap().len(), 1);

        let provenance_path = fixture
            .store_root
            .join("runs")
            .join(&fixture.authority_run_id.0)
            .join("provenance.json");
        let mut wrong_provenance = record.expected_provenance();
        wrong_provenance.input_sha256 = "f".repeat(64);
        ProjectStore::write_json(&provenance_path, &vec![wrong_provenance]).unwrap();
        assert!(
            reopened.list_reviews(&fixture.project_id).is_err(),
            "review-specific provenance tamper was accepted"
        );
        ProjectStore::write_json(&provenance_path, &vec![record.expected_provenance()]).unwrap();
        assert_eq!(reopened.list_reviews(&fixture.project_id).unwrap().len(), 1);

        // A bare filesystem write cannot mint a second authoritative review:
        // its claimed authority run does not exist and list therefore fails
        // closed instead of returning the forged JSON.
        let mut forged = record;
        forged.review_id = "op-review-forged".into();
        forged.operation_id = "op-review-forged".into();
        forged.authority_run_id = "forged-authority-run".into();
        let forged_path = reopened.review_path(&fixture.project_id, "op-review-forged");
        ProjectStore::write_json(&forged_path, &forged).unwrap();
        assert!(reopened.list_reviews(&fixture.project_id).is_err());
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
            tampered
                .project_store
                .apply_mutation(&review_request(&tampered, "op-review-tamper"))
                .is_err(),
            "tampered bytes produced a review"
        );
        assert!(
            tampered
                .project_store
                .list_reviews(&tampered.project_id)
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
        assert!(clean.project_store.apply_mutation(&unknown).is_err());

        let mut wrong_session = review_request(&clean, "op-review-session");
        wrong_session.session_id = "session-2".into();
        assert!(matches!(
            clean.project_store.apply_mutation(&wrong_session),
            Err(ScienceError::Ownership)
        ));

        let mut forged_reviewer = review_request(&clean, "op-review-reviewer");
        if let ProjectMutation::ReviewRecord { reviewer_id, .. } = &mut forged_reviewer.mutation {
            *reviewer_id = "Nature-Reviewer-2".into();
        }
        assert!(matches!(
            clean.project_store.apply_mutation(&forged_reviewer),
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
            clean.project_store.apply_mutation(&wrong_project),
            Err(ScienceError::Ownership)
        ));

        let mut traversal = review_request(&clean, "op-review-traversal");
        if let ProjectMutation::ReviewRecord { project_id, .. } = &mut traversal.mutation {
            *project_id = ProjectId("../operations".into());
        }
        traversal.expected_revision = None;
        let error = clean.project_store.apply_mutation(&traversal).unwrap_err();
        assert!(
            matches!(&error, ScienceError::Invalid(message) if message.contains("projectId")),
            "project-id traversal failed for the wrong reason: {error}"
        );

        #[cfg(unix)]
        {
            let linked = fixture();
            let linked_request = review_request(&linked, "op-review-symlink");
            let outcome = linked
                .project_store
                .apply_mutation(&linked_request)
                .unwrap();
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
                    .list_reviews(&linked.project_id)
                    .is_err(),
                "a symlinked review ledger entry was followed"
            );
        }
    }
}
