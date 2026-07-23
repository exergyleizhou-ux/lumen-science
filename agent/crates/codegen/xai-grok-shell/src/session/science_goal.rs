//! Goal × Expert fencing for durable Science completion.
//!
//! Consultant output is advisory only. The sole completion path in this
//! module is [`ScienceGoalReview::host_verify_and_complete`], which binds the
//! current Goal generation and Expert task to a byte-reverified durable
//! Science run before asking the existing [`GoalTracker`] to complete.

use sha2::{Digest, Sha256};

use super::expert::{
    CallbackGuard, ExpertErrorCode, ExpertModeState, ExpertPhase, HostVerificationOutcome,
    VerificationSummary,
};
use super::goal_tracker::{GoalStatus, GoalTracker};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScienceGoalBinding {
    pub goal_id: String,
    pub goal_verifier_id: String,
    pub run_id: xai_grok_science::RunId,
    pub expert_task_id: String,
    pub expert_generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsultantCapability {
    Advisory,
    ConnectorApproval,
    TransportExecution,
    GoalCompletion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScienceGoalReviewError {
    NoActiveGoal,
    NoActiveExpert,
    StaleBinding,
    StaleCallback,
    ConsultantAuthorityDenied,
    HostVerificationFailed,
    AuditPersistenceFailed,
    GoalCompletionRejected,
}

#[derive(Debug, Clone)]
pub struct ScienceGoalReview {
    binding: ScienceGoalBinding,
}

impl ScienceGoalReview {
    pub fn bind(
        goal: &GoalTracker,
        expert: &ExpertModeState,
        run_id: xai_grok_science::RunId,
    ) -> Result<Self, ScienceGoalReviewError> {
        let snapshot = goal
            .snapshot()
            .filter(|goal| goal.status == GoalStatus::Active)
            .ok_or(ScienceGoalReviewError::NoActiveGoal)?;
        let expert_task_id = expert
            .task_id
            .clone()
            .filter(|_| expert.is_active())
            .ok_or(ScienceGoalReviewError::NoActiveExpert)?;
        Ok(Self {
            binding: ScienceGoalBinding {
                goal_id: snapshot.goal_id.clone(),
                goal_verifier_id: snapshot.verifier_id.clone(),
                run_id,
                expert_task_id,
                expert_generation: expert.task_generation,
            },
        })
    }

    pub fn binding(&self) -> &ScienceGoalBinding {
        &self.binding
    }

    /// Record a consultant callback as advisory evidence only. Even a literal
    /// `PASS` cannot mutate Goal or Science state.
    pub fn record_consultant_advisory(
        &self,
        goal: &GoalTracker,
        expert: &ExpertModeState,
        guard: &CallbackGuard,
        advisory: &str,
        store: &xai_grok_science::ScienceStore,
    ) -> Result<(), ScienceGoalReviewError> {
        if let Err(error) = self.validate_live_binding(goal, expert) {
            self.audit_rejection(store, "stale_binding")?;
            return Err(error);
        }
        if expert.accept_callback(guard) == Err(ExpertErrorCode::StaleCallback) {
            self.audit_rejection(store, "stale_callback")?;
            return Err(ScienceGoalReviewError::StaleCallback);
        }
        let advisory_sha256 = sha256(advisory.as_bytes());
        store
            .append_event(
                &self.binding.run_id,
                "science.goal_review",
                "review.consultant_advisory",
                serde_json::json!({
                    "binding_sha256": self.binding_sha256(),
                    "advisory_sha256": advisory_sha256,
                    "authority": "advisory_only"
                }),
            )
            .map_err(|_| ScienceGoalReviewError::AuditPersistenceFailed)?;
        Ok(())
    }

    /// The type-level authority boundary used by consultant tool adapters.
    /// Only advisory output is accepted; approval, execution, and Goal
    /// completion are host-owned operations.
    pub fn authorize_consultant_capability(
        capability: ConsultantCapability,
    ) -> Result<(), ScienceGoalReviewError> {
        match capability {
            ConsultantCapability::Advisory => Ok(()),
            ConsultantCapability::ConnectorApproval
            | ConsultantCapability::TransportExecution
            | ConsultantCapability::GoalCompletion => {
                Err(ScienceGoalReviewError::ConsultantAuthorityDenied)
            }
        }
    }

    pub fn host_verify_and_complete(
        &self,
        goal: &mut GoalTracker,
        expert: &mut ExpertModeState,
        store: &xai_grok_science::ScienceStore,
    ) -> Result<xai_grok_science::review::HostVerificationReport, ScienceGoalReviewError> {
        if let Err(error) = self.validate_live_binding(goal, expert) {
            self.audit_rejection(store, "stale_binding")?;
            return Err(error);
        }
        if expert.phase != ExpertPhase::HostVerifying {
            self.audit_rejection(store, "wrong_expert_phase")?;
            return Err(ScienceGoalReviewError::HostVerificationFailed);
        }
        let report =
            xai_grok_science::review::verify_for_goal_completion(store, &self.binding.run_id)
                .map_err(|_| ScienceGoalReviewError::HostVerificationFailed)?;

        store
            .append_event(
                &self.binding.run_id,
                "science.host_verification",
                "review.host_verification_met",
                serde_json::json!({
                    "binding_sha256": self.binding_sha256(),
                    "verification_sha256": report.verification_sha256,
                    "artifact_count": report.artifact_count,
                    "evidence_count": report.evidence_count,
                    "provenance_count": report.provenance_count
                }),
            )
            .map_err(|_| ScienceGoalReviewError::AuditPersistenceFailed)?;

        expert.verification_summary = VerificationSummary {
            outcome: HostVerificationOutcome::Met,
            tests_run: 1,
            tests_passed: 1,
            build_ran: false,
            build_passed: false,
            permission_or_sandbox_failure: false,
            summary: "durable Science evidence verified by host".into(),
            workspace_fingerprint: Some(report.verification_sha256.clone()),
            expert_task_id: Some(self.binding.expert_task_id.clone()),
            generation: Some(self.binding.expert_generation),
            executor_pass: Some("science_host_verification".into()),
        };
        if !goal.complete() {
            return Err(ScienceGoalReviewError::GoalCompletionRejected);
        }
        Ok(report)
    }

    fn validate_live_binding(
        &self,
        goal: &GoalTracker,
        expert: &ExpertModeState,
    ) -> Result<(), ScienceGoalReviewError> {
        let snapshot = goal
            .snapshot()
            .ok_or(ScienceGoalReviewError::StaleBinding)?;
        if snapshot.goal_id != self.binding.goal_id
            || snapshot.verifier_id != self.binding.goal_verifier_id
            || snapshot.status != GoalStatus::Active
            || !expert.is_active()
            || expert.task_id.as_deref() != Some(self.binding.expert_task_id.as_str())
            || expert.task_generation != self.binding.expert_generation
        {
            return Err(ScienceGoalReviewError::StaleBinding);
        }
        Ok(())
    }

    fn audit_rejection(
        &self,
        store: &xai_grok_science::ScienceStore,
        reason_code: &str,
    ) -> Result<(), ScienceGoalReviewError> {
        store
            .append_event(
                &self.binding.run_id,
                "science.goal_review",
                "review.callback_rejected",
                serde_json::json!({
                    "binding_sha256": self.binding_sha256(),
                    "reason_code": reason_code
                }),
            )
            .map(|_| ())
            .map_err(|_| ScienceGoalReviewError::AuditPersistenceFailed)
    }

    fn binding_sha256(&self) -> String {
        let mut digest = Sha256::new();
        digest.update(b"lumen-science-goal-binding-v1\0");
        digest.update(self.binding.goal_id.as_bytes());
        digest.update([0]);
        digest.update(self.binding.goal_verifier_id.as_bytes());
        digest.update([0]);
        digest.update(self.binding.run_id.0.as_bytes());
        digest.update([0]);
        digest.update(self.binding.expert_task_id.as_bytes());
        digest.update(self.binding.expert_generation.to_le_bytes());
        format!("{:x}", digest.finalize())
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use chrono::Utc;

    use super::*;
    use crate::session::expert::{ExpertFeatureState, ExpertMode};
    use xai_grok_science::{
        Approval, ApprovalDecision, CallId, Evidence, ProjectId, Provenance, RunContext, RunState,
    };

    fn goal_and_expert(root: &std::path::Path) -> (GoalTracker, ExpertModeState) {
        let mut goal = GoalTracker::new(root.join("session"));
        goal.create_goal(
            "goal-science-1".into(),
            "verify the science result".into(),
            None,
            0,
            Utc::now().to_rfc3339(),
            None,
        );
        let mut expert = ExpertModeState::configured();
        expert
            .start("review science evidence", ExpertMode::Deep, "test-executor")
            .unwrap();
        (goal, expert)
    }

    fn completed_science(
        root: &std::path::Path,
    ) -> (xai_grok_science::ScienceStore, xai_grok_science::RunId) {
        let store = xai_grok_science::ScienceStore::new(root.join("science-store"));
        let run_id = xai_grok_science::RunId::new_v7();
        let project = ProjectId::new("science-project");
        let owner = "science-owner";
        store
            .create_run(RunContext {
                run_id: run_id.clone(),
                project_id: project.clone(),
                session_id: "session".into(),
                owner_id: owner.into(),
                workspace_root: root.to_path_buf(),
                provider: "offline".into(),
                approval_policy: "production-session-permission".into(),
                tool_profile: "science-review".into(),
                artifact_root: root.join("artifacts"),
                environment: BTreeMap::new(),
            })
            .unwrap();
        let call = CallId::new(uuid::Uuid::now_v7().to_string());
        store
            .request_approval(Approval {
                project_id: project.clone(),
                run_id: run_id.clone(),
                call_id: call.clone(),
                owner_id: owner.into(),
                decision: ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        store
            .decide_approval(&project, &run_id, owner, &call, ApprovalDecision::Allow)
            .unwrap();
        let artifact = store
            .put_artifact(
                &project,
                &run_id,
                owner,
                call,
                Path::new("result.txt"),
                b"science result",
                "text/plain",
                "result",
            )
            .unwrap();
        store
            .add_evidence(Evidence {
                run_id: run_id.clone(),
                claim: "result".into(),
                source: "fixture".into(),
                artifact_sha256: Some(artifact.sha256.clone()),
                verified_at: Utc::now(),
            })
            .unwrap();
        store
            .add_provenance(Provenance {
                run_id: run_id.clone(),
                source_uri: "fixture://review".into(),
                source_commit: None,
                source_path: None,
                license: "test-only".into(),
                retrieved_at: Utc::now(),
                input_sha256: artifact.sha256,
                tool: "fixture".into(),
                environment: BTreeMap::new(),
            })
            .unwrap();
        store.transition(&run_id, RunState::Running, None).unwrap();
        store
            .transition(&run_id, RunState::Succeeded, None)
            .unwrap();
        (store, run_id)
    }

    #[test]
    fn consultant_pass_is_advisory_and_cannot_complete_goal() {
        let root = tempfile::tempdir().unwrap();
        let (goal, mut expert) = goal_and_expert(root.path());
        expert.phase = ExpertPhase::PreparingEvidence;
        let guard = expert.reserve_consult(1, 1).unwrap();
        let (store, run_id) = completed_science(root.path());
        let review = ScienceGoalReview::bind(&goal, &expert, run_id).unwrap();
        review
            .record_consultant_advisory(&goal, &expert, &guard, "PASS", &store)
            .unwrap();
        assert_eq!(goal.status(), Some(GoalStatus::Active));
        assert_eq!(
            store.load_run(&review.binding().run_id).unwrap().state,
            RunState::Succeeded
        );
    }

    #[test]
    fn stale_callback_after_new_goal_or_restart_is_rejected_and_audited() {
        let root = tempfile::tempdir().unwrap();
        let (mut goal, mut expert) = goal_and_expert(root.path());
        expert.phase = ExpertPhase::PreparingEvidence;
        let guard = expert.reserve_consult(1, 1).unwrap();
        let (store, run_id) = completed_science(root.path());
        let review = ScienceGoalReview::bind(&goal, &expert, run_id.clone()).unwrap();
        goal.create_goal(
            "replacement".into(),
            "replacement".into(),
            None,
            0,
            Utc::now().to_rfc3339(),
            None,
        );
        assert_eq!(
            review.record_consultant_advisory(&goal, &expert, &guard, "PASS", &store),
            Err(ScienceGoalReviewError::StaleBinding)
        );

        let (goal, expert) = goal_and_expert(root.path());
        let review = ScienceGoalReview::bind(&goal, &expert, run_id.clone()).unwrap();
        let recovered = expert.recover_after_crash();
        assert_eq!(
            review.validate_live_binding(&goal, &recovered),
            Err(ScienceGoalReviewError::StaleBinding)
        );
        let events = store.events_after(&run_id, 0, 100).unwrap();
        assert!(
            events
                .iter()
                .any(|event| event.kind == "review.callback_rejected")
        );
        assert!(
            events
                .iter()
                .all(|event| !event.payload.to_string().contains("PASS"))
        );
    }

    #[test]
    fn consultant_has_no_approval_transport_or_completion_authority() {
        assert!(
            ScienceGoalReview::authorize_consultant_capability(ConsultantCapability::Advisory)
                .is_ok()
        );
        for denied in [
            ConsultantCapability::ConnectorApproval,
            ConsultantCapability::TransportExecution,
            ConsultantCapability::GoalCompletion,
        ] {
            assert_eq!(
                ScienceGoalReview::authorize_consultant_capability(denied),
                Err(ScienceGoalReviewError::ConsultantAuthorityDenied)
            );
        }
    }

    #[test]
    fn matching_host_verification_completes_only_the_bound_goal() {
        let root = tempfile::tempdir().unwrap();
        let (mut goal, mut expert) = goal_and_expert(root.path());
        let (store, run_id) = completed_science(root.path());
        let review = ScienceGoalReview::bind(&goal, &expert, run_id).unwrap();
        expert.phase = ExpertPhase::HostVerifying;
        let report = review
            .host_verify_and_complete(&mut goal, &mut expert, &store)
            .unwrap();
        assert_eq!(goal.status(), Some(GoalStatus::Complete));
        assert_eq!(
            expert.verification_summary.outcome,
            HostVerificationOutcome::Met
        );
        assert_eq!(
            expert.verification_summary.workspace_fingerprint.as_deref(),
            Some(report.verification_sha256.as_str())
        );

        let (mut replacement, mut same_expert) = goal_and_expert(root.path());
        let stale = ScienceGoalReview {
            binding: review.binding.clone(),
        };
        same_expert.phase = ExpertPhase::HostVerifying;
        assert_eq!(
            stale.host_verify_and_complete(&mut replacement, &mut same_expert, &store),
            Err(ScienceGoalReviewError::StaleBinding)
        );
        assert_eq!(replacement.status(), Some(GoalStatus::Active));
    }

    #[test]
    fn audit_surfaces_are_hash_only_and_secret_free() {
        let root = tempfile::tempdir().unwrap();
        let (goal, mut expert) = goal_and_expert(root.path());
        expert.phase = ExpertPhase::PreparingEvidence;
        let guard = expert.reserve_consult(1, 1).unwrap();
        let (store, run_id) = completed_science(root.path());
        let review = ScienceGoalReview::bind(&goal, &expert, run_id.clone()).unwrap();
        let secret = "Bearer secret-provider-payload";
        review
            .record_consultant_advisory(&goal, &expert, &guard, secret, &store)
            .unwrap();
        let events = store.events_after(&run_id, 0, 100).unwrap();
        let serialized = serde_json::to_string(&events).unwrap();
        assert!(!serialized.contains(secret));
        assert!(!serialized.contains("secret-provider-payload"));
        assert!(serialized.contains(&sha256(secret.as_bytes())));
        assert_eq!(expert.feature_state, ExpertFeatureState::Active);
    }
}
