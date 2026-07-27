//! SessionActor-gated project mutations.
//!
//! Every write to a `ResearchProject`, its claims, or its evidence graph that
//! originates outside this process must arrive through here, so that it
//! carries the three things a bare `ProjectStore` call cannot:
//!
//! - an **operation id**, making a retry idempotent instead of duplicating a
//!   project or claim;
//! - **session/owner/project binding**, so one session cannot mutate another's
//!   project by naming it;
//! - an **expected revision**, so a mutation computed against a stale read is
//!   rejected instead of silently clobbering a concurrent update.
//!
//! The ACP adapter must not call `ProjectStore` mutators directly: the actor
//! owns approval and the durable run record, and this type is what it applies
//! once permission has been granted.

use super::claim::Claim;
use super::model::{ProjectId, ProjectStatus, ResearchProject};
use super::store::ProjectStore;
use crate::{Result, ScienceError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The mutations the ACP surface is allowed to ask for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectMutation {
    ProjectCreate {
        title: String,
        research_question: String,
    },
    ProjectTransition {
        project_id: ProjectId,
        status: ProjectStatus,
    },
    /// Refine the research question of an existing project.
    ///
    /// A question is rarely right on day one — the product must allow
    /// refinement, and routing it here (rather than editing desktop-side
    /// state) keeps the durable record the single authority on what is being
    /// asked. Same ownership check, permission prompt, idempotency and
    /// revision CAS as every other mutation.
    QuestionUpdate {
        project_id: ProjectId,
        research_question: String,
    },
    ClaimPropose {
        project_id: ProjectId,
        statement: String,
        proposed_by: String,
    },
    EvidenceAttach {
        project_id: ProjectId,
        claim_id: String,
        artifact_sha256: String,
        label: String,
        run_id: Option<String>,
    },
}

impl ProjectMutation {
    /// Stable label used in run events and the idempotency ledger.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ProjectCreate { .. } => "project_create",
            Self::ProjectTransition { .. } => "project_transition",
            Self::QuestionUpdate { .. } => "question_update",
            Self::ClaimPropose { .. } => "claim_propose",
            Self::EvidenceAttach { .. } => "evidence_attach",
        }
    }

    /// The project this mutation targets, or `None` when it creates one.
    pub fn target_project(&self) -> Option<&ProjectId> {
        match self {
            Self::ProjectCreate { .. } => None,
            Self::ProjectTransition { project_id, .. }
            | Self::QuestionUpdate { project_id, .. }
            | Self::ClaimPropose { project_id, .. }
            | Self::EvidenceAttach { project_id, .. } => Some(project_id),
        }
    }
}

/// One admitted mutation request: what to do, on whose authority, and against
/// which revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationRequest {
    /// Caller-chosen idempotency key. A repeat of the same id returns the
    /// first outcome instead of applying the mutation twice.
    pub operation_id: String,
    /// The session that is allowed to replay this operation id.
    pub session_id: String,
    pub owner_id: String,
    /// Revision the caller computed this mutation against. `None` opts out of
    /// the compare-and-swap; `Some` fails the request if the project moved.
    /// Must be `None` for `ProjectCreate`, which has no prior revision.
    pub expected_revision: Option<String>,
    pub mutation: ProjectMutation,
}

/// The result of applying (or replaying) a mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationOutcome {
    pub operation_id: String,
    pub kind: String,
    pub project_id: ProjectId,
    /// Project revision *after* the mutation; feed it back as the next
    /// `expected_revision`.
    pub revision: String,
    pub result: serde_json::Value,
    /// True when this came from the idempotency ledger rather than a fresh
    /// application.
    pub replayed: bool,
}

/// A durable record that an operation id has already been applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationRecord {
    pub operation_id: String,
    pub session_id: String,
    pub owner_id: String,
    pub kind: String,
    pub project_id: ProjectId,
    pub revision: String,
    pub result: serde_json::Value,
    pub completed_at: DateTime<Utc>,
}

/// Operation ids address a file, so keep them boring and bounded.
pub(crate) fn validate_operation_id(operation_id: &str) -> Result<()> {
    if !(8..=128).contains(&operation_id.len()) {
        return Err(ScienceError::Invalid(
            "operationId must be 8..=128 characters".into(),
        ));
    }
    if !operation_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(ScienceError::Invalid(
            "operationId must be [A-Za-z0-9_-] only".into(),
        ));
    }
    Ok(())
}

impl ProjectStore {
    /// Apply a mutation under session/owner/project binding, idempotency, and
    /// compare-and-swap. Holds the store write guard for the whole sequence,
    /// so the revision check and the mutation cannot be split by a concurrent
    /// writer.
    pub fn apply_mutation(&self, request: &MutationRequest) -> Result<MutationOutcome> {
        validate_operation_id(&request.operation_id)?;
        if request.session_id.is_empty() || request.owner_id.is_empty() {
            return Err(ScienceError::Invalid(
                "mutation requires a session id and owner id".into(),
            ));
        }
        if matches!(request.mutation, ProjectMutation::ProjectCreate { .. })
            && request.expected_revision.is_some()
        {
            return Err(ScienceError::Invalid(
                "project_create cannot carry an expectedRevision".into(),
            ));
        }

        let _guard = self.write_guard()?;

        // Idempotency: a replay returns the first outcome, and only to the
        // session and owner that produced it.
        if let Some(record) = self.lookup_operation(&request.operation_id)? {
            if record.session_id != request.session_id || record.owner_id != request.owner_id {
                return Err(ScienceError::Ownership);
            }
            if record.kind != request.mutation.kind() {
                return Err(ScienceError::Invalid(format!(
                    "operation {} was already applied as {}, not {}",
                    request.operation_id,
                    record.kind,
                    request.mutation.kind()
                )));
            }
            return Ok(MutationOutcome {
                operation_id: record.operation_id,
                kind: record.kind,
                project_id: record.project_id,
                revision: record.revision,
                result: record.result,
                replayed: true,
            });
        }

        // Binding + compare-and-swap against the target project.
        if let Some(project_id) = request.mutation.target_project() {
            let project = self.load_project(project_id)?;
            if project.owner_id.0 != request.owner_id {
                return Err(ScienceError::Ownership);
            }
            if let Some(expected) = &request.expected_revision {
                let current = self.project_revision(project_id)?;
                if &current != expected {
                    return Err(ScienceError::Invalid(format!(
                        "revision conflict on project {}: expected {expected}, found {current}",
                        project_id.0
                    )));
                }
            }
        }

        let (project_id, result) = self.apply_mutation_inner(request)?;
        let revision = self.project_revision(&project_id)?;
        let record = OperationRecord {
            operation_id: request.operation_id.clone(),
            session_id: request.session_id.clone(),
            owner_id: request.owner_id.clone(),
            kind: request.mutation.kind().to_string(),
            project_id: project_id.clone(),
            revision: revision.clone(),
            result: result.clone(),
            completed_at: Utc::now(),
        };
        self.record_operation(&record)?;
        Ok(MutationOutcome {
            operation_id: record.operation_id,
            kind: record.kind,
            project_id,
            revision,
            result,
            replayed: false,
        })
    }

    fn apply_mutation_inner(
        &self,
        request: &MutationRequest,
    ) -> Result<(ProjectId, serde_json::Value)> {
        match &request.mutation {
            ProjectMutation::ProjectCreate {
                title,
                research_question,
            } => {
                if title.is_empty() || research_question.is_empty() {
                    return Err(ScienceError::Invalid(
                        "project_create requires a title and research question".into(),
                    ));
                }
                let project: ResearchProject = self.create_project_inner(
                    &request.owner_id,
                    title.clone(),
                    research_question.clone(),
                )?;
                let id = project.project_id.clone();
                Ok((id, serde_json::to_value(project)?))
            }
            ProjectMutation::ProjectTransition { project_id, status } => {
                let project = self.transition_project_inner(
                    project_id,
                    &request.owner_id,
                    *status,
                )?;
                Ok((project_id.clone(), serde_json::to_value(project)?))
            }
            ProjectMutation::QuestionUpdate {
                project_id,
                research_question,
            } => {
                if research_question.is_empty() {
                    return Err(ScienceError::Invalid(
                        "question_update requires a research question".into(),
                    ));
                }
                let project = self.update_question_inner(
                    project_id,
                    &request.owner_id,
                    research_question.clone(),
                )?;
                Ok((project_id.clone(), serde_json::to_value(project)?))
            }
            ProjectMutation::ClaimPropose {
                project_id,
                statement,
                proposed_by,
            } => {
                if statement.is_empty() {
                    return Err(ScienceError::Invalid(
                        "claim_propose requires a statement".into(),
                    ));
                }
                let claim: Claim = self.propose_claim_inner(
                    project_id,
                    &request.owner_id,
                    statement.clone(),
                    proposed_by.clone(),
                )?;
                Ok((project_id.clone(), serde_json::to_value(claim)?))
            }
            ProjectMutation::EvidenceAttach {
                project_id,
                claim_id,
                artifact_sha256,
                label,
                run_id,
            } => {
                let (claim, graph) = self.attach_evidence_inner(
                    project_id,
                    &request.owner_id,
                    claim_id,
                    artifact_sha256.clone(),
                    label.clone(),
                    run_id.clone(),
                )?;
                Ok((
                    project_id.clone(),
                    serde_json::json!({
                        "claim": claim,
                        "nodeCount": graph.nodes.len(),
                        "edgeCount": graph.edges.len(),
                    }),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_request(operation_id: &str) -> MutationRequest {
        MutationRequest {
            operation_id: operation_id.into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            expected_revision: None,
            mutation: ProjectMutation::ProjectCreate {
                title: "Demo".into(),
                research_question: "Does EcoRI cut?".into(),
            },
        }
    }

    #[test]
    fn create_is_idempotent_under_one_operation_id() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let request = create_request("op-create-0001");

        let first = store.apply_mutation(&request).unwrap();
        assert!(!first.replayed);
        let second = store.apply_mutation(&request).unwrap();
        assert!(second.replayed, "retry created a second project");
        assert_eq!(first.project_id, second.project_id);
        assert_eq!(first.revision, second.revision);
        assert_eq!(store.list_projects().unwrap().len(), 1);
    }

    #[test]
    fn replay_is_refused_to_a_different_session_or_owner() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let request = create_request("op-create-0002");
        store.apply_mutation(&request).unwrap();

        let mut other_session = request.clone();
        other_session.session_id = "session-2".into();
        assert!(matches!(
            store.apply_mutation(&other_session),
            Err(ScienceError::Ownership)
        ));

        let mut other_owner = request.clone();
        other_owner.owner_id = "owner-2".into();
        assert!(matches!(
            store.apply_mutation(&other_owner),
            Err(ScienceError::Ownership)
        ));
    }

    #[test]
    fn operation_id_cannot_be_reused_for_a_different_mutation() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let created = store.apply_mutation(&create_request("op-create-0003")).unwrap();

        let mut reuse = create_request("op-create-0003");
        reuse.mutation = ProjectMutation::ClaimPropose {
            project_id: created.project_id.clone(),
            statement: "s".into(),
            proposed_by: "sci".into(),
        };
        let error = store.apply_mutation(&reuse).unwrap_err();
        assert!(
            matches!(&error, ScienceError::Invalid(m) if m.contains("already applied")),
            "unexpected: {error}"
        );
    }

    #[test]
    fn expected_revision_rejects_a_stale_mutation() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let created = store.apply_mutation(&create_request("op-create-0004")).unwrap();
        let stale = created.revision.clone();

        let first_claim = MutationRequest {
            operation_id: "op-claim-00001".into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            expected_revision: Some(stale.clone()),
            mutation: ProjectMutation::ClaimPropose {
                project_id: created.project_id.clone(),
                statement: "first".into(),
                proposed_by: "sci".into(),
            },
        };
        let applied = store.apply_mutation(&first_claim).unwrap();
        assert_ne!(applied.revision, stale, "revision must advance");

        // A second mutation computed against the pre-claim revision is stale.
        let second_claim = MutationRequest {
            operation_id: "op-claim-00002".into(),
            expected_revision: Some(stale),
            mutation: ProjectMutation::ClaimPropose {
                project_id: created.project_id.clone(),
                statement: "second".into(),
                proposed_by: "sci".into(),
            },
            ..first_claim.clone()
        };
        let error = store.apply_mutation(&second_claim).unwrap_err();
        assert!(
            matches!(&error, ScienceError::Invalid(m) if m.contains("revision conflict")),
            "unexpected: {error}"
        );

        // The same mutation against the current revision succeeds.
        let fresh = MutationRequest {
            expected_revision: Some(applied.revision.clone()),
            ..second_claim
        };
        store.apply_mutation(&fresh).unwrap();
    }

    #[test]
    fn question_update_persists_and_is_owner_gated() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let created = store.apply_mutation(&create_request("op-create-0006")).unwrap();
        let project_id = created.project_id.clone();

        let update = MutationRequest {
            operation_id: "op-question-0001".into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            expected_revision: Some(created.revision.clone()),
            mutation: ProjectMutation::QuestionUpdate {
                project_id: project_id.clone(),
                research_question: "How does the refined question persist?".into(),
            },
        };
        let outcome = store.apply_mutation(&update).unwrap();
        assert_ne!(outcome.revision, created.revision, "revision must advance");

        // Read back through the DURABLE record, not the outcome echo: the
        // claim under test is persistence, and an echo can lie about that.
        let reloaded = store.load_project(&project_id).unwrap();
        assert_eq!(
            reloaded.research_question,
            "How does the refined question persist?"
        );

        // A non-owner cannot rewrite someone else's question.
        let intruder = MutationRequest {
            operation_id: "op-question-0002".into(),
            owner_id: "owner-9".into(),
            expected_revision: None,
            mutation: ProjectMutation::QuestionUpdate {
                project_id: project_id.clone(),
                research_question: "hijacked".into(),
            },
            ..update.clone()
        };
        assert!(matches!(
            store.apply_mutation(&intruder),
            Err(ScienceError::Ownership)
        ));

        // And an empty question is refused rather than recorded.
        let empty = MutationRequest {
            operation_id: "op-question-0003".into(),
            mutation: ProjectMutation::QuestionUpdate {
                project_id,
                research_question: String::new(),
            },
            ..update
        };
        assert!(store.apply_mutation(&empty).is_err());
    }

    #[test]
    fn mutation_refuses_a_project_owned_by_someone_else() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let created = store.apply_mutation(&create_request("op-create-0005")).unwrap();

        let intruder = MutationRequest {
            operation_id: "op-intruder-01".into(),
            session_id: "session-9".into(),
            owner_id: "owner-9".into(),
            expected_revision: None,
            mutation: ProjectMutation::ProjectTransition {
                project_id: created.project_id,
                status: ProjectStatus::Planned,
            },
        };
        assert!(matches!(
            store.apply_mutation(&intruder),
            Err(ScienceError::Ownership)
        ));
    }

    #[test]
    fn operation_id_shape_is_validated() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        for bad in ["", "short", "../../escape", "has space", &"x".repeat(129)] {
            let request = create_request(bad);
            assert!(
                store.apply_mutation(&request).is_err(),
                "operation id {bad:?} was accepted"
            );
        }
    }

    #[test]
    fn full_mutation_chain_applies_through_one_api() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let created = store.apply_mutation(&create_request("op-chain-00001")).unwrap();
        let project_id = created.project_id.clone();

        let transitioned = store
            .apply_mutation(&MutationRequest {
                operation_id: "op-chain-00002".into(),
                session_id: "session-1".into(),
                owner_id: "owner-1".into(),
                expected_revision: Some(created.revision),
                mutation: ProjectMutation::ProjectTransition {
                    project_id: project_id.clone(),
                    status: ProjectStatus::Planned,
                },
            })
            .unwrap();

        let claim = store
            .apply_mutation(&MutationRequest {
                operation_id: "op-chain-00003".into(),
                session_id: "session-1".into(),
                owner_id: "owner-1".into(),
                expected_revision: Some(transitioned.revision),
                mutation: ProjectMutation::ClaimPropose {
                    project_id: project_id.clone(),
                    statement: "EcoRI site present".into(),
                    proposed_by: "sci".into(),
                },
            })
            .unwrap();
        let claim_id = claim.result["claim_id"].as_str().unwrap().to_string();

        let sha = "a".repeat(64);
        store
            .register_artifact(&project_id, "owner-1", sha.clone(), "art", None)
            .unwrap();

        let attached = store
            .apply_mutation(&MutationRequest {
                operation_id: "op-chain-00004".into(),
                session_id: "session-1".into(),
                owner_id: "owner-1".into(),
                // register_artifact moved the revision, so re-read it.
                expected_revision: Some(store.project_revision(&project_id).unwrap()),
                mutation: ProjectMutation::EvidenceAttach {
                    project_id: project_id.clone(),
                    claim_id: claim_id.clone(),
                    artifact_sha256: sha,
                    label: "seq".into(),
                    run_id: Some("run-1".into()),
                },
            })
            .unwrap();
        assert_eq!(attached.result["edgeCount"], 1);
        assert_eq!(
            store.load_claim(&project_id, &claim_id).unwrap().status,
            super::super::claim::ClaimStatus::EvidenceAttached
        );
    }

    #[test]
    fn every_write_moves_the_revision() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let created = store.apply_mutation(&create_request("op-rev-000001")).unwrap();
        let project_id = created.project_id.clone();

        let mut seen = std::collections::BTreeSet::new();
        seen.insert(created.revision.clone());
        assert_eq!(store.project_revision(&project_id).unwrap(), created.revision);

        store
            .register_artifact(&project_id, "owner-1", "b".repeat(64), "art", None)
            .unwrap();
        assert!(seen.insert(store.project_revision(&project_id).unwrap()));

        store
            .propose_claim(&project_id, "owner-1", "s", "sci")
            .unwrap();
        assert!(seen.insert(store.project_revision(&project_id).unwrap()));
    }
}
