//! Host-owned completion verification for Science runs.
//!
//! This module deliberately has no model, Goal, Expert, permission, or
//! transport handles.  It can only inspect durable Science records and
//! registered artifact bytes.  Shell/Goal code may use a successful report as
//! one input to completion; consultant text is never an input here.

use sha2::{Digest, Sha256};

use crate::{ApprovalDecision, RunId, RunState, ScienceError, ScienceStore};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HostVerificationReport {
    pub run_id: RunId,
    pub state: RunState,
    pub approval_count: usize,
    pub artifact_count: usize,
    pub evidence_count: usize,
    pub provenance_count: usize,
    /// Stable digest of the verified durable record identities and artifact
    /// hashes. It contains no provider payload, credential, path, or hostname.
    pub verification_sha256: String,
}

/// Verify the durable evidence required before a Science-backed Goal may be
/// completed. Every registered artifact is reopened through the store's
/// ownership/traversal guard and hashed again from bytes.
pub fn verify_for_goal_completion(
    store: &ScienceStore,
    run_id: &RunId,
) -> Result<HostVerificationReport, ScienceError> {
    let run = store.load_run(run_id)?;
    if run.state != RunState::Succeeded {
        return Err(ScienceError::Invalid(
            "science completion requires a succeeded run".into(),
        ));
    }

    let approvals = store.approvals(run_id)?;
    if approvals.is_empty()
        || approvals
            .iter()
            .any(|approval| approval.decision != ApprovalDecision::Allow)
    {
        return Err(ScienceError::Invalid(
            "science completion requires only terminal allow approvals".into(),
        ));
    }

    let artifacts = store.artifacts(run_id)?;
    let evidence = store.evidence(run_id)?;
    let provenance = store.provenance(run_id)?;
    if artifacts.is_empty() || evidence.is_empty() || provenance.is_empty() {
        return Err(ScienceError::Invalid(
            "science completion requires artifact, evidence, and provenance".into(),
        ));
    }

    let mut artifact_hashes = std::collections::BTreeSet::new();
    for artifact in &artifacts {
        let bytes = store.artifact_bytes(
            &run.context.project_id,
            run_id,
            &run.context.owner_id,
            &artifact.relative_path,
        )?;
        if bytes.len() as u64 != artifact.bytes || crate::hex_sha256(&bytes) != artifact.sha256 {
            return Err(ScienceError::Invalid(
                "registered science artifact hash or length mismatch".into(),
            ));
        }
        artifact_hashes.insert(artifact.sha256.as_str());
    }

    let mut cited_artifact = false;
    if evidence.iter().any(|item| {
        let bad_hash = item.artifact_sha256.as_deref().is_some_and(|hash| {
            cited_artifact = true;
            !artifact_hashes.contains(hash)
        });
        item.run_id != *run_id
            || item.claim.trim().is_empty()
            || item.source.trim().is_empty()
            || bad_hash
    }) || !cited_artifact
    {
        return Err(ScienceError::Invalid(
            "science evidence must cite a registered artifact".into(),
        ));
    }
    if provenance.iter().any(|item| {
        item.run_id != *run_id
            || item.source_uri.trim().is_empty()
            || item.license.trim().is_empty()
            || item.tool.trim().is_empty()
            || !is_sha256(&item.input_sha256)
    }) {
        return Err(ScienceError::Invalid(
            "science provenance is incomplete or malformed".into(),
        ));
    }

    let mut digest = Sha256::new();
    digest.update(b"lumen-science-host-verification-v1\0");
    digest.update(run_id.0.as_bytes());
    digest.update([0]);
    for approval in &approvals {
        digest.update(approval.call_id.0.as_bytes());
        digest.update([0]);
    }
    for artifact in &artifacts {
        digest.update(artifact.sha256.as_bytes());
        digest.update([0]);
    }
    for item in &evidence {
        digest.update(item.artifact_sha256.as_deref().unwrap().as_bytes());
        digest.update([0]);
    }
    for item in &provenance {
        digest.update(item.input_sha256.as_bytes());
        digest.update([0]);
    }

    Ok(HostVerificationReport {
        run_id: run_id.clone(),
        state: run.state,
        approval_count: approvals.len(),
        artifact_count: artifacts.len(),
        evidence_count: evidence.len(),
        provenance_count: provenance.len(),
        verification_sha256: format!("{:x}", digest.finalize()),
    })
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use chrono::Utc;

    use super::*;
    use crate::{Approval, CallId, Evidence, ProjectId, Provenance, RunContext};

    fn completed_fixture() -> (tempfile::TempDir, ScienceStore, RunId) {
        let root = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(root.path().join("store"));
        let run_id = RunId::new_v7();
        let project_id = ProjectId::new("review-project");
        let owner = "review-owner";
        store
            .create_run(RunContext {
                run_id: run_id.clone(),
                project_id: project_id.clone(),
                session_id: "review-session".into(),
                owner_id: owner.into(),
                workspace_root: root.path().to_path_buf(),
                provider: "offline-deterministic".into(),
                approval_policy: "production-session-permission".into(),
                tool_profile: "science-review-test".into(),
                artifact_root: root.path().join("artifacts"),
                environment: BTreeMap::new(),
            })
            .unwrap();
        let call_id = CallId::new(uuid::Uuid::now_v7().to_string());
        store
            .request_approval(Approval {
                project_id: project_id.clone(),
                run_id: run_id.clone(),
                call_id: call_id.clone(),
                owner_id: owner.into(),
                decision: ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        store
            .decide_approval(
                &project_id,
                &run_id,
                owner,
                &call_id,
                ApprovalDecision::Allow,
            )
            .unwrap();
        let artifact = store
            .put_artifact(
                &project_id,
                &run_id,
                owner,
                call_id,
                Path::new("result.txt"),
                b"verified science bytes",
                "text/plain",
                "verified result",
            )
            .unwrap();
        store
            .add_evidence(Evidence {
                run_id: run_id.clone(),
                claim: "fixture result".into(),
                source: "host verification fixture".into(),
                artifact_sha256: Some(artifact.sha256.clone()),
                verified_at: Utc::now(),
            })
            .unwrap();
        store
            .add_provenance(Provenance {
                run_id: run_id.clone(),
                source_uri: "fixture://science-review".into(),
                source_commit: None,
                source_path: None,
                license: "test-only".into(),
                retrieved_at: Utc::now(),
                input_sha256: artifact.sha256,
                tool: "science-review-fixture".into(),
                environment: BTreeMap::new(),
            })
            .unwrap();
        store.transition(&run_id, RunState::Running, None).unwrap();
        store
            .transition(&run_id, RunState::Succeeded, None)
            .unwrap();
        (root, store, run_id)
    }

    #[test]
    fn durable_evidence_is_required_and_rehashed() {
        let (_root, store, run_id) = completed_fixture();
        let report = verify_for_goal_completion(&store, &run_id).unwrap();
        assert_eq!(report.state, RunState::Succeeded);
        assert_eq!(report.approval_count, 1);
        assert_eq!(report.artifact_count, 1);
        assert_eq!(report.evidence_count, 1);
        assert_eq!(report.provenance_count, 1);
        assert!(is_sha256(&report.verification_sha256));

        let artifact = store.artifacts(&run_id).unwrap().remove(0);
        std::fs::write(
            store
                .root
                .join("runs")
                .join(&run_id.0)
                .join("artifacts")
                .join(artifact.relative_path),
            b"tampered",
        )
        .unwrap();
        assert!(verify_for_goal_completion(&store, &run_id).is_err());
    }

    #[test]
    fn non_succeeded_or_incomplete_records_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(root.path().join("store"));
        let run_id = RunId::new_v7();
        store
            .create_run(RunContext {
                run_id: run_id.clone(),
                project_id: ProjectId::new("p"),
                session_id: "s".into(),
                owner_id: "o".into(),
                workspace_root: root.path().to_path_buf(),
                provider: "offline".into(),
                approval_policy: "host".into(),
                tool_profile: "review".into(),
                artifact_root: root.path().join("artifacts"),
                environment: BTreeMap::new(),
            })
            .unwrap();
        assert!(verify_for_goal_completion(&store, &run_id).is_err());
    }
}
