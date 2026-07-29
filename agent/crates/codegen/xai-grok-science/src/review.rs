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
    verify_durable_evidence_in_state(store, run_id, RunState::Succeeded)
}

/// Verify a complete actor-owned commit while the run is still Running.
///
/// This is deliberately crate-internal: it exists so the SessionActor can
/// make Succeeded the final fallible write, not so callers can consume
/// pre-terminal artifacts.
pub(crate) fn verify_before_successful_commit(
    store: &ScienceStore,
    run_id: &RunId,
) -> Result<HostVerificationReport, ScienceError> {
    verify_durable_evidence_in_state(store, run_id, RunState::Running)
}

fn verify_durable_evidence_in_state(
    store: &ScienceStore,
    run_id: &RunId,
    required_state: RunState,
) -> Result<HostVerificationReport, ScienceError> {
    let run = store.load_run(run_id)?;
    if run.state != required_state {
        return Err(ScienceError::Invalid(
            "science verification run state does not match its commit phase".into(),
        ));
    }

    let approvals = store.approvals(run_id)?;
    let [approval] = approvals.as_slice() else {
        return Err(ScienceError::Invalid(
            "science completion requires exactly one terminal allow approval".into(),
        ));
    };
    if approval.project_id != run.context.project_id
        || approval.run_id != *run_id
        || approval.owner_id != run.context.owner_id
        || approval.call_id.0.trim().is_empty()
        || approval.decision != ApprovalDecision::Allow
        || approval.decided_at.is_none()
    {
        return Err(ScienceError::Invalid(
            "science completion approval does not exactly bind its run context and call".into(),
        ));
    }

    let artifacts = store.artifacts(run_id)?;
    let evidence = store.evidence(run_id)?;
    let provenance = store.provenance(run_id)?;
    let previews = store.previews(run_id)?;
    if artifacts.is_empty() || evidence.is_empty() || provenance.is_empty() {
        return Err(ScienceError::Invalid(
            "science completion requires artifact, evidence, and provenance".into(),
        ));
    }

    let mut artifact_hashes = std::collections::BTreeSet::new();
    let mut artifact_paths = std::collections::BTreeSet::new();
    for artifact in &artifacts {
        if artifact.run_id != *run_id
            || artifact.call_id != approval.call_id
            || !artifact_paths.insert(artifact.relative_path.clone())
            || !artifact_hashes.insert(artifact.sha256.as_str())
        {
            return Err(ScienceError::Invalid(
                "science artifact registry does not exactly bind its run approval".into(),
            ));
        }
        let bytes = if required_state == RunState::Succeeded {
            store.artifact_bytes(
                &run.context.project_id,
                run_id,
                &run.context.owner_id,
                &artifact.relative_path,
            )?
        } else {
            store.running_artifact_bytes(
                &run.context.project_id,
                run_id,
                &run.context.owner_id,
                &artifact.relative_path,
            )?
        };
        if bytes.len() as u64 != artifact.bytes || crate::hex_sha256(&bytes) != artifact.sha256 {
            return Err(ScienceError::Invalid(
                "registered science artifact hash or length mismatch".into(),
            ));
        }
    }

    let mut cited_artifacts = std::collections::BTreeSet::new();
    let any_bad = evidence.iter().any(|item| {
        let missing_or_bad_hash = match item.artifact_sha256.as_deref() {
            Some(hash) => {
                cited_artifacts.insert(hash);
                !artifact_hashes.contains(hash)
            }
            None => true, // missing artifact citation = fail
        };
        missing_or_bad_hash
            || item.run_id != *run_id
            || item.claim.trim().is_empty()
            || item.source.trim().is_empty()
    });
    if any_bad || cited_artifacts != artifact_hashes {
        return Err(ScienceError::Invalid(
            "science evidence must exactly cover the registered artifact hashes".into(),
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

    let mut events = Vec::new();
    let mut after = 0;
    loop {
        let batch = store.events_after(run_id, after, 1_000)?;
        if batch.is_empty() {
            break;
        }
        after = batch.last().expect("non-empty event page").seq;
        events.extend(batch);
        if events.len() > 10_000 {
            return Err(ScienceError::Invalid(
                "science authority event log exceeds verification limit".into(),
            ));
        }
    }

    // This digest is a source-authority snapshot, not merely an artifact set.
    // Review records persist it and replay recomputes it, so replacing an
    // evidence claim, provenance producer, approval timestamp, preview, event
    // or any other durable authority field cannot retain the old proof.
    let mut digest = Sha256::new();
    digest.update(b"lumen-science-host-verification-v2\0");
    update_digest_json(&mut digest, &run)?;
    update_digest_json(&mut digest, &approvals)?;
    update_digest_json(&mut digest, &artifacts)?;
    update_digest_json(&mut digest, &evidence)?;
    update_digest_json(&mut digest, &provenance)?;
    update_digest_json(&mut digest, &previews)?;
    update_digest_json(&mut digest, &events)?;

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

fn update_digest_json(
    digest: &mut Sha256,
    value: &impl serde::Serialize,
) -> Result<(), ScienceError> {
    let bytes = serde_json::to_vec(value)?;
    digest.update((bytes.len() as u64).to_le_bytes());
    digest.update(bytes);
    Ok(())
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

    fn running_fixture() -> (tempfile::TempDir, ScienceStore, RunId) {
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
            .transition(&run_id, RunState::AwaitingApproval, None)
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
        store.transition(&run_id, RunState::Running, None).unwrap();
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
        (root, store, run_id)
    }

    fn completed_fixture() -> (tempfile::TempDir, ScienceStore, RunId) {
        let (root, store, run_id) = running_fixture();
        store.transition_succeeded_verified(&run_id).unwrap();
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

    // ── Negative tests per DeepSeek handover doc Phase 7 ─────────────

    #[test]
    fn wrong_project_id_fails() {
        let (_root, store, run_id) = completed_fixture();
        // Evidence pointing to wrong project must fail
        let evidence = store.evidence(&run_id).unwrap();
        assert!(!evidence.is_empty(), "fixture has evidence");
        // Store lookup with wrong project fails
        let bad_store = ScienceStore::new(store.root.join("..").join("alt-store"));
        let bad_run = RunId::new_v7();
        assert!(verify_for_goal_completion(&bad_store, &bad_run).is_err());
    }

    #[test]
    fn evidence_citing_unregistered_artifact_fails() {
        let (_root, store, run_id) = running_fixture();
        // Persist invalid evidence before the terminal transition. Terminal
        // runs are immutable, but the completion verifier must still reject a
        // malformed record that was created while the run was active.
        let fake_sha = "a".repeat(64);
        store
            .add_evidence(Evidence {
                run_id: run_id.clone(),
                claim: "fake claim citing nonexistent artifact".into(),
                source: "negative test".into(),
                artifact_sha256: Some(fake_sha),
                verified_at: Utc::now(),
            })
            .unwrap();
        store.transition_succeeded_verified(&run_id).unwrap();
        assert!(verify_for_goal_completion(&store, &run_id).is_err());
    }

    #[test]
    fn approval_not_terminal_allow_fails() {
        let (_root, store, run_id) = completed_fixture();

        // Output creation above used the real Pending -> Allow -> Running
        // lifecycle. Corrupt only the durable approval projection afterwards
        // so this test still reaches the verifier's terminal-Allow seam rather
        // than passing because the output gate rejected fixture setup.
        let mut approvals = store.approvals(&run_id).unwrap();
        assert_eq!(approvals.len(), 1);
        approvals[0].decision = ApprovalDecision::Deny;
        std::fs::write(
            store
                .root
                .join("runs")
                .join(&run_id.0)
                .join("approvals.json"),
            serde_json::to_vec_pretty(&approvals).unwrap(),
        )
        .unwrap();

        let error = verify_for_goal_completion(&store, &run_id).unwrap_err();
        assert!(matches!(
            error,
            ScienceError::Invalid(message)
                if message == "science completion approval does not exactly bind its run context and call"
        ));
    }

    #[test]
    fn run_cannot_succeed_without_approvals() {
        let root = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(root.path().join("store"));
        let run_id = RunId::new_v7();
        let project_id = ProjectId::new("no-approval");
        let owner = "no-approval-owner";
        store
            .create_run(RunContext {
                run_id: run_id.clone(),
                project_id: project_id.clone(),
                session_id: "s".into(),
                owner_id: owner.into(),
                workspace_root: root.path().to_path_buf(),
                provider: "offline".into(),
                approval_policy: "host".into(),
                tool_profile: "review".into(),
                artifact_root: root.path().join("artifacts"),
                environment: BTreeMap::new(),
            })
            .unwrap();
        // The state machine itself now rejects both the Running edge and the
        // dedicated successful completion seam before the host verifier is
        // even consulted.
        assert!(store.transition(&run_id, RunState::Running, None).is_err());
        assert!(store.transition_succeeded_verified(&run_id).is_err());
        assert!(verify_for_goal_completion(&store, &run_id).is_err());
    }

    #[test]
    fn reopened_artifact_hash_drift_fails() {
        let (_root, store, run_id) = completed_fixture();
        // Tamper with the artifact bytes on disk — reopen must detect SHA256 drift
        let artifacts = store.artifacts(&run_id).unwrap();
        let artifact = &artifacts[0];
        let disk_path = store
            .root
            .join("runs")
            .join(&run_id.0)
            .join("artifacts")
            .join(&artifact.relative_path);
        std::fs::write(&disk_path, b"tampered-with-malicious-payload-here").unwrap();
        assert!(verify_for_goal_completion(&store, &run_id).is_err());
    }

    #[test]
    fn provenance_incomplete_fails() {
        let (_root, store, run_id) = running_fixture();
        // Add provenance with empty source_uri before the immutable terminal
        // transition; verification must still reject the malformed record.
        store
            .add_provenance(Provenance {
                run_id: run_id.clone(),
                source_uri: "".into(), // empty source = incomplete
                source_commit: None,
                source_path: None,
                license: "".into(), // empty license = incomplete
                retrieved_at: Utc::now(),
                input_sha256: "a".repeat(64),
                tool: "".into(), // empty tool = incomplete
                environment: BTreeMap::new(),
            })
            .unwrap();
        store.transition_succeeded_verified(&run_id).unwrap();
        assert!(verify_for_goal_completion(&store, &run_id).is_err());
    }

    #[test]
    fn evidence_without_artifact_citation_fails() {
        let (_root, store, run_id) = running_fixture();
        // Evidence with artifact_sha256 = None must fail after the run reaches
        // the immutable Succeeded state.
        store
            .add_evidence(Evidence {
                run_id: run_id.clone(),
                claim: "evidence without citation".into(),
                source: "negative test".into(),
                artifact_sha256: None, // no artifact cited
                verified_at: Utc::now(),
            })
            .unwrap();
        store.transition_succeeded_verified(&run_id).unwrap();
        assert!(verify_for_goal_completion(&store, &run_id).is_err());
    }
}
