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
use super::review_store::ReviewVerdict;
use super::store::ProjectStore;
use crate::features::ScienceFeature;
use crate::{Result, ScienceError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The mutations the ACP surface is allowed to ask for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectMutation {
    ProjectCreate {
        title: String,
        research_question: String,
    },
    ProjectMigrate {
        source_run_id: String,
        title: String,
        research_question: String,
        /// Minted by the SessionActor. The ACP adapter sends an empty value;
        /// replay fingerprints deliberately normalize it because a retry
        /// initially receives a fresh candidate run id before the actor
        /// recovers the original migration commit.
        #[serde(default)]
        authority_run_id: String,
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
    ReviewRecord {
        project_id: ProjectId,
        reviewer_id: String,
        verdict: ReviewVerdict,
        summary: String,
        claim_id: Option<String>,
        source_run_id: String,
        authority_run_id: String,
        artifact_sha256s: Vec<String>,
    },
}

impl ProjectMutation {
    /// Stable label used in run events and the idempotency ledger.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ProjectCreate { .. } => "project_create",
            Self::ProjectMigrate { .. } => "project_migrate",
            Self::ProjectTransition { .. } => "project_transition",
            Self::QuestionUpdate { .. } => "question_update",
            Self::ClaimPropose { .. } => "claim_propose",
            Self::EvidenceAttach { .. } => "evidence_attach",
            Self::ReviewRecord { .. } => "review_record",
        }
    }

    /// The project this mutation targets, or `None` when it creates one.
    pub fn target_project(&self) -> Option<&ProjectId> {
        match self {
            Self::ProjectCreate { .. } | Self::ProjectMigrate { .. } => None,
            Self::ProjectTransition { project_id, .. }
            | Self::QuestionUpdate { project_id, .. }
            | Self::ClaimPropose { project_id, .. }
            | Self::EvidenceAttach { project_id, .. }
            | Self::ReviewRecord { project_id, .. } => Some(project_id),
        }
    }

    /// Capabilities that must all be enabled before the SessionActor may
    /// create a durable run or ask the operator to approve this mutation.
    pub fn required_features(&self) -> &'static [ScienceFeature] {
        use ScienceFeature::{
            ClaimLifecycle, Collaboration, EvidenceGraph, MigrationChain, ResearchProject,
            ReviewPackage,
        };
        match self {
            Self::ProjectCreate { .. }
            | Self::ProjectTransition { .. }
            | Self::QuestionUpdate { .. } => &[ResearchProject],
            Self::ProjectMigrate { .. } => &[
                ResearchProject,
                MigrationChain,
                EvidenceGraph,
                ClaimLifecycle,
            ],
            Self::ClaimPropose { .. } => &[ResearchProject, ClaimLifecycle, EvidenceGraph],
            Self::EvidenceAttach { .. } => &[ResearchProject, EvidenceGraph, ClaimLifecycle],
            Self::ReviewRecord { .. } => &[
                ResearchProject,
                EvidenceGraph,
                ClaimLifecycle,
                Collaboration,
                ReviewPackage,
            ],
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
    /// Hash of the exact authority-bearing request, excluding only the
    /// actor-minted review run id. Older records do not have enough information
    /// to prove replay equivalence and therefore fail closed instead of
    /// silently replaying a different same-kind request.
    #[serde(default)]
    pub request_sha256: String,
    pub project_id: ProjectId,
    pub revision: String,
    pub result: serde_json::Value,
    pub completed_at: DateTime<Utc>,
}

impl MutationRequest {
    /// Stable binding used by the idempotency ledger.
    ///
    /// The original optimistic revision is authority-bearing: a caller must
    /// replay the same admitted compare-and-swap request, not replace it with a
    /// looser precondition after success. The review authority run id is minted
    /// by the actor on each attempted call, so only that field is normalized.
    pub fn replay_fingerprint(&self) -> Result<String> {
        let mut mutation = self.mutation.clone();
        if let ProjectMutation::ReviewRecord {
            authority_run_id, ..
        } = &mut mutation
        {
            authority_run_id.clear();
        }
        if let ProjectMutation::ProjectMigrate {
            authority_run_id, ..
        } = &mut mutation
        {
            authority_run_id.clear();
        }
        let binding = serde_json::json!({
            "operation_id": self.operation_id,
            "session_id": self.session_id,
            "owner_id": self.owner_id,
            "expected_revision": self.expected_revision,
            "mutation": mutation,
        });
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&binding)?)
        ))
    }

    /// Project id reserved by one migration request.
    ///
    /// Random target ids make a crash between the project commit and the
    /// operation ledger create an orphan and let a retry create a second
    /// project. Deriving the id from the normalized authority request gives
    /// every retry exactly one recovery target.
    pub fn migration_target_project_id(&self) -> Result<Option<ProjectId>> {
        if !matches!(self.mutation, ProjectMutation::ProjectMigrate { .. }) {
            return Ok(None);
        }
        Ok(Some(ProjectId(format!(
            "migrated-{}",
            self.replay_fingerprint()?
        ))))
    }
}

impl OperationRecord {
    /// Prove that a caller is replaying the same request, not merely reusing
    /// the same operation id for another target or payload of the same kind.
    pub fn verify_replay(&self, request: &MutationRequest) -> Result<()> {
        if self.session_id != request.session_id || self.owner_id != request.owner_id {
            return Err(ScienceError::Ownership);
        }
        if self.kind != request.mutation.kind() {
            return Err(ScienceError::Invalid(format!(
                "operation {} was already applied as {}, not {}",
                request.operation_id,
                self.kind,
                request.mutation.kind()
            )));
        }
        if self.request_sha256.is_empty() {
            return Err(ScienceError::Invalid(format!(
                "operation {} predates request-bound replay; use a new operationId",
                request.operation_id
            )));
        }
        if self.request_sha256 != request.replay_fingerprint()? {
            return Err(ScienceError::Invalid(format!(
                "operation {} replay does not match its original request",
                request.operation_id
            )));
        }
        Ok(())
    }
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

fn validate_migration_request_shape(request: &MutationRequest) -> Result<()> {
    validate_operation_id(&request.operation_id)?;
    let ProjectMutation::ProjectMigrate {
        source_run_id,
        title,
        research_question,
        authority_run_id,
    } = &request.mutation
    else {
        return Err(ScienceError::Invalid(
            "migration authority requires project_migrate".into(),
        ));
    };
    if request.session_id.is_empty()
        || request.owner_id.is_empty()
        || request.expected_revision.is_some()
        || source_run_id.is_empty()
        || title.is_empty()
        || research_question.is_empty()
        || authority_run_id.is_empty()
        || request.migration_target_project_id()?.is_none()
    {
        return Err(ScienceError::Invalid(
            "project_migrate requires exact session/owner/source/metadata/authority identity and no expectedRevision"
                .into(),
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
        if matches!(request.mutation, ProjectMutation::ProjectMigrate { .. }) {
            return Err(ScienceError::Invalid(
                "project_migrate requires a SessionActor-verified source bundle".into(),
            ));
        }
        if matches!(request.mutation, ProjectMutation::ReviewRecord { .. }) {
            return Err(ScienceError::Invalid(
                "review_record requires SessionActor-retained ScienceStore authority".into(),
            ));
        }
        self.apply_mutation_inner_admitted(request, None)
    }

    /// Apply a review only through the exact `ScienceStore` capability the
    /// SessionActor retained before permission. This prevents the project
    /// mutation layer from reopening a pathname after Allow and binds the
    /// succeeded source run to the same retained root as the project ledger.
    pub fn apply_actor_review(
        &self,
        request: &MutationRequest,
        science_store: &crate::ScienceStore,
        admission: &super::review_store::ReviewAdmission,
    ) -> Result<MutationOutcome> {
        let ProjectMutation::ReviewRecord { project_id, .. } = &request.mutation else {
            return Err(ScienceError::Invalid(
                "review authority cannot authorize another mutation kind".into(),
            ));
        };
        validate_operation_id(&request.operation_id)?;
        if request.session_id.is_empty() || request.owner_id.is_empty() {
            return Err(ScienceError::Invalid(
                "review mutation requires a session id and owner id".into(),
            ));
        }
        if !science_store.shares_root_capability_with(self)? {
            return Err(ScienceError::Invalid(
                "review ScienceStore and ProjectStore retained different roots".into(),
            ));
        }

        let _guard = self.write_guard()?;
        if let Some(record) = self.lookup_operation(&request.operation_id)? {
            record.verify_replay(request)?;
            if record.kind != "review_record" {
                return Err(ScienceError::Invalid(
                    "review admission cannot replay another mutation kind".into(),
                ));
            }
            let review: super::review_store::ReviewRecord =
                serde_json::from_value(record.result.clone())?;
            self.verify_review_replay_admission(science_store, request, admission, &review)?;
            return Ok(MutationOutcome {
                operation_id: record.operation_id,
                kind: record.kind,
                project_id: record.project_id,
                revision: record.revision,
                result: record.result,
                replayed: true,
            });
        }

        let project = self.load_project(project_id)?;
        if project.owner_id.0 != request.owner_id {
            return Err(ScienceError::Ownership);
        }
        let current_revision = self.project_revision(project_id)?;
        if let Some(expected) = request
            .expected_revision
            .as_deref()
            .filter(|expected| *expected != current_revision)
        {
            return Err(ScienceError::Invalid(format!(
                "revision conflict on project {}: expected {expected}, found {current_revision}",
                project_id.0
            )));
        }

        // This is the authority seam: while the same project write lock is
        // held, recapture every pre-Allow input and require exact admission
        // equality immediately before the ledger record is created.
        admission.verify_after_allow_locked(self, science_store, request, &current_revision)?;
        let review =
            self.record_review_inner(science_store, request, admission, &current_revision)?;
        let revision = self.project_revision(project_id)?;
        let result = serde_json::to_value(review)?;
        let record = OperationRecord {
            operation_id: request.operation_id.clone(),
            session_id: request.session_id.clone(),
            owner_id: request.owner_id.clone(),
            kind: request.mutation.kind().to_string(),
            request_sha256: request.replay_fingerprint()?,
            project_id: project_id.clone(),
            revision: revision.clone(),
            result: result.clone(),
            completed_at: Utc::now(),
        };
        self.record_operation(&record)?;
        Ok(MutationOutcome {
            operation_id: record.operation_id,
            kind: record.kind,
            project_id: project_id.clone(),
            revision,
            result,
            replayed: false,
        })
    }

    /// Repair only the operation-ledger half of an already-written actor review.
    ///
    /// The opaque grant is minted while the original authority is still
    /// Running and before its manifest/evidence/provenance commit. Recovery
    /// reopens and verifies the exact v3 review under the same project write
    /// lock, then writes no project data other than the missing operation
    /// record.
    pub fn recover_actor_review_operation(
        &self,
        request: &MutationRequest,
        grant: &super::review_store::ReviewRecoveryGrant,
    ) -> Result<MutationOutcome> {
        let ProjectMutation::ReviewRecord { .. } = &request.mutation else {
            return Err(ScienceError::Invalid(
                "review recovery grant cannot authorize another mutation kind".into(),
            ));
        };
        validate_operation_id(&request.operation_id)?;
        if request.session_id.is_empty() || request.owner_id.is_empty() {
            return Err(ScienceError::Invalid(
                "review recovery requires a session id and owner id".into(),
            ));
        }

        let _guard = self.write_guard()?;
        let (review, revision) = grant.revalidate_locked(self, request)?;
        let result = serde_json::to_value(&review)?;
        if let Some(record) = self.lookup_operation(&request.operation_id)? {
            record.verify_replay(request)?;
            if record.kind != "review_record"
                || record.project_id != review.project_id
                || record.revision != revision
                || record.result != result
            {
                return Err(ScienceError::Invalid(
                    "existing review operation is not an exact orphan recovery replay".into(),
                ));
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

        let record = OperationRecord {
            operation_id: request.operation_id.clone(),
            session_id: request.session_id.clone(),
            owner_id: request.owner_id.clone(),
            kind: request.mutation.kind().to_string(),
            request_sha256: request.replay_fingerprint()?,
            project_id: review.project_id.clone(),
            revision: revision.clone(),
            result: result.clone(),
            completed_at: Utc::now(),
        };
        self.record_operation(&record)?;
        Ok(MutationOutcome {
            operation_id: record.operation_id,
            kind: record.kind,
            project_id: record.project_id,
            revision,
            result,
            replayed: true,
        })
    }

    /// Apply the one project mutation whose source bytes live in
    /// `ScienceStore`. The bundle's fields cannot be constructed outside the
    /// science crate; it is produced only by revalidating an actor admission
    /// after durable Allow.
    pub fn apply_actor_migration(
        &self,
        request: &MutationRequest,
        admission: &super::migration::MigrationAdmission,
        source: &super::migration::VerifiedMigrationBundle,
    ) -> Result<MutationOutcome> {
        validate_migration_request_shape(request)?;
        // Cross-store order is ProjectStore -> ScienceStore everywhere. Other
        // product paths already hold the project guard while committing
        // Science outputs; reversing it here would permit an ABBA deadlock.
        let _guard = self.write_guard()?;
        source.with_live_authority_for_project_store(self, || {
            // Validate the exact request/admission/bundle tuple even when an
            // operation record already exists. The generic idempotency fast
            // path must not let an unrelated live bundle retrieve another
            // migration's result.
            if self.lookup_operation(&request.operation_id)?.is_some()
                && self
                    .lookup_migration_commit(&request.operation_id)?
                    .is_none()
            {
                return Err(ScienceError::Invalid(
                    "existing migration operation is missing its immutable commit journal".into(),
                ));
            }
            self.admit_migration_commit_inner(request, admission, source)?;
            source.verify_target_copies_for_project_store(self)?;
            source.mark_project_commit_fence()?;
            let outcome =
                self.apply_mutation_inner_admitted_guarded(request, Some((admission, source)))?;
            if outcome.replayed {
                self.verify_migration_result(
                    request,
                    &serde_json::from_value(outcome.result.clone())?,
                )?;
            }
            Ok(outcome)
        })
    }

    /// Durably reserve the exact migration operation before any target
    /// artifact or project record is written.
    ///
    /// `source` is an opaque capability minted only from a Running authority
    /// run with one durable Allow. A retry either reopens the same journal or
    /// fails closed if any request/admission byte differs.
    pub fn admit_actor_migration(
        &self,
        request: &MutationRequest,
        admission: &super::migration::MigrationAdmission,
        source: &super::migration::VerifiedMigrationBundle,
    ) -> Result<super::migration::MigrationCommit> {
        validate_migration_request_shape(request)?;
        let _guard = self.write_guard()?;
        source.with_live_authority_for_project_store(self, || {
            self.admit_migration_commit_inner(request, admission, source)
        })
    }

    /// Adopt a project bundle whose actor commit journal landed before the
    /// generic operation record.
    ///
    /// The recovery grant is minted only after `ScienceStore` reopens the
    /// original Running/Succeeded authority run, its durable Allow, and every
    /// target-owned copied artifact. No new permission or target is created.
    pub fn recover_actor_migration_operation(
        &self,
        request: &MutationRequest,
        grant: &super::migration::MigrationRecoveryGrant,
    ) -> Result<MutationOutcome> {
        validate_migration_request_shape(request)?;
        let _guard = self.write_guard()?;
        grant.with_revalidated_authority_for_project_store(self, |authority_state| {
            let commit = self
                .lookup_migration_commit(&request.operation_id)?
                .ok_or_else(|| {
                    ScienceError::Invalid(format!(
                        "migration commit {} is missing",
                        request.operation_id
                    ))
                })?;
            grant.verify_retained_commit(&commit)?;
            let request_sha256 = request.replay_fingerprint()?;
            if grant.operation_id() != request.operation_id
                || grant.request_sha256() != request_sha256
                || grant.target_project_id() != &commit.manifest.target_project_id
                || grant.authority_run_id() != &commit.manifest.authority_run_id
                || commit.request_sha256 != request_sha256
            {
                return Err(ScienceError::Ownership);
            }
            if let Some(record) = self.lookup_operation(&request.operation_id)? {
                if authority_state == crate::RunState::Running {
                    grant.mark_project_commit_fence()?;
                }
                record.verify_replay(request)?;
                let expected_result =
                    serde_json::to_value(super::migration::MigrationResult::from_commit(&commit)?)?;
                if record.kind != "project_migrate"
                    || record.project_id != commit.manifest.target_project_id
                    || record.result != expected_result
                {
                    return Err(ScienceError::Invalid(
                        "existing migration operation differs from its retained authority commit"
                            .into(),
                    ));
                }
                self.verify_migration_result(
                    request,
                    &serde_json::from_value(record.result.clone())?,
                )?;
                return Ok(MutationOutcome {
                    operation_id: record.operation_id,
                    kind: record.kind,
                    project_id: record.project_id,
                    revision: record.revision,
                    result: record.result,
                    replayed: true,
                });
            }
            if authority_state != crate::RunState::Running {
                return Err(ScienceError::Invalid(
                    "a terminal migration authority may verify replay but cannot publish missing project records"
                        .into(),
                ));
            }
            grant.mark_project_commit_fence()?;
            let result = self.resume_migration_commit_inner(request, &commit)?;
            let project_id = result.target_project_id.clone();
            let revision = self.project_revision(&project_id)?;
            let result = serde_json::to_value(result)?;
            let record = OperationRecord {
                operation_id: request.operation_id.clone(),
                session_id: request.session_id.clone(),
                owner_id: request.owner_id.clone(),
                kind: request.mutation.kind().into(),
                request_sha256,
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
                replayed: true,
            })
        })
    }

    fn apply_mutation_inner_admitted(
        &self,
        request: &MutationRequest,
        migration: Option<(
            &super::migration::MigrationAdmission,
            &super::migration::VerifiedMigrationBundle,
        )>,
    ) -> Result<MutationOutcome> {
        validate_operation_id(&request.operation_id)?;
        if request.session_id.is_empty() || request.owner_id.is_empty() {
            return Err(ScienceError::Invalid(
                "mutation requires a session id and owner id".into(),
            ));
        }
        if matches!(
            request.mutation,
            ProjectMutation::ProjectCreate { .. } | ProjectMutation::ProjectMigrate { .. }
        ) && request.expected_revision.is_some()
        {
            return Err(ScienceError::Invalid(
                "project creation cannot carry an expectedRevision".into(),
            ));
        }

        let _guard = self.write_guard()?;
        self.apply_mutation_inner_admitted_guarded(request, migration)
    }

    /// Apply while the caller retains this ProjectStore's write guard.
    ///
    /// Migration uses this entry after acquiring the guard before the Science
    /// authority lock, preserving the repository-wide Project -> Science lock
    /// order without re-entering the non-reentrant project mutex.
    fn apply_mutation_inner_admitted_guarded(
        &self,
        request: &MutationRequest,
        migration: Option<(
            &super::migration::MigrationAdmission,
            &super::migration::VerifiedMigrationBundle,
        )>,
    ) -> Result<MutationOutcome> {
        // Idempotency: a replay returns the first outcome, and only to the
        // session and owner that produced it.
        if let Some(record) = self.lookup_operation(&request.operation_id)? {
            record.verify_replay(request)?;
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

        let (project_id, result) = self.apply_mutation_inner(request, migration)?;
        let revision = self.project_revision(&project_id)?;
        let record = OperationRecord {
            operation_id: request.operation_id.clone(),
            session_id: request.session_id.clone(),
            owner_id: request.owner_id.clone(),
            kind: request.mutation.kind().to_string(),
            request_sha256: request.replay_fingerprint()?,
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
        migration: Option<(
            &super::migration::MigrationAdmission,
            &super::migration::VerifiedMigrationBundle,
        )>,
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
            ProjectMutation::ProjectMigrate {
                source_run_id,
                title,
                research_question,
                authority_run_id,
            } => {
                if source_run_id.is_empty()
                    || title.is_empty()
                    || research_question.is_empty()
                    || authority_run_id.is_empty()
                {
                    return Err(ScienceError::Invalid(
                        "project_migrate requires a source run, title, research question, and actor authority run"
                            .into(),
                    ));
                }
                let (admission, migration) = migration.ok_or_else(|| {
                    ScienceError::Invalid(
                        "project_migrate requires a SessionActor-verified source bundle".into(),
                    )
                })?;
                let migration = self.migrate_v1_to_v2_inner(request, admission, migration)?;
                let id = migration.target_project_id.clone();
                Ok((id, serde_json::to_value(migration)?))
            }
            ProjectMutation::ProjectTransition { project_id, status } => {
                let project =
                    self.transition_project_inner(project_id, &request.owner_id, *status)?;
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
            ProjectMutation::ReviewRecord { .. } => Err(ScienceError::Invalid(
                "review_record requires an immutable ReviewAdmission".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Approval, ApprovalDecision, CallId, Evidence, ProjectId as ScienceProjectId, Provenance,
        RunContext, RunId, RunState, ScienceStore,
    };
    use chrono::Utc;
    use std::{collections::BTreeMap, path::Path};
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

    fn verified_migration(
        root: &Path,
        request: &MutationRequest,
    ) -> (
        super::super::migration::MigrationAdmission,
        super::super::migration::VerifiedMigrationBundle,
    ) {
        let ProjectMutation::ProjectMigrate {
            source_run_id,
            title,
            research_question,
            authority_run_id,
        } = &request.mutation
        else {
            panic!("migration fixture requires ProjectMigrate");
        };
        let science_store = ScienceStore::new(root);
        let source_run = RunId::new(source_run_id);
        let source_project = ScienceProjectId::new("legacy-project");
        let source_call = CallId::new("legacy-call");
        science_store
            .create_run(RunContext {
                run_id: source_run.clone(),
                project_id: source_project.clone(),
                session_id: request.session_id.clone(),
                owner_id: request.owner_id.clone(),
                workspace_root: root.to_path_buf(),
                provider: "offline".into(),
                approval_policy: "test".into(),
                tool_profile: "legacy-v1".into(),
                artifact_root: root.join("runs"),
                environment: BTreeMap::new(),
            })
            .unwrap();
        science_store
            .request_approval(Approval {
                project_id: source_project.clone(),
                run_id: source_run.clone(),
                call_id: source_call.clone(),
                owner_id: request.owner_id.clone(),
                decision: ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        science_store
            .transition(&source_run, RunState::AwaitingApproval, None)
            .unwrap();
        science_store
            .decide_approval(
                &source_project,
                &source_run,
                &request.owner_id,
                &source_call,
                ApprovalDecision::Allow,
            )
            .unwrap();
        science_store
            .transition(&source_run, RunState::Running, None)
            .unwrap();
        let artifact = science_store
            .put_artifact(
                &source_project,
                &source_run,
                &request.owner_id,
                source_call,
                Path::new("legacy.json"),
                br#"{"verified":true}"#,
                "application/json",
                "legacy result",
            )
            .unwrap();
        science_store
            .add_evidence(Evidence {
                run_id: source_run.clone(),
                claim: "The legacy result is preserved.".into(),
                source: "legacy.json".into(),
                artifact_sha256: Some(artifact.sha256.clone()),
                verified_at: Utc::now(),
            })
            .unwrap();
        science_store
            .add_provenance(Provenance {
                run_id: source_run.clone(),
                source_uri: "fixture://legacy.json".into(),
                source_commit: None,
                source_path: Some("legacy.json".into()),
                license: "CC0-1.0".into(),
                retrieved_at: Utc::now(),
                input_sha256: artifact.sha256,
                tool: "migration-test".into(),
                environment: BTreeMap::new(),
            })
            .unwrap();
        science_store
            .transition_succeeded_verified(&source_run)
            .unwrap();
        let target = request.migration_target_project_id().unwrap().unwrap();
        let mut authority_context = RunContext {
            run_id: RunId::new(authority_run_id),
            project_id: ScienceProjectId::new(target.0.clone()),
            session_id: request.session_id.clone(),
            owner_id: request.owner_id.clone(),
            workspace_root: root.to_path_buf(),
            provider: "offline".into(),
            approval_policy: "test".into(),
            tool_profile: "project-migration".into(),
            artifact_root: root.join("runs"),
            environment: BTreeMap::new(),
        };
        let admission = super::super::migration::MigrationAdmission::capture(
            &science_store,
            &authority_context,
            source_run,
            request.operation_id.clone(),
            target,
            RunId::new(authority_run_id),
            title,
            research_question,
        )
        .unwrap();
        authority_context.environment.insert(
            "project_migration_admission_sha256".into(),
            admission.sha256().unwrap(),
        );
        science_store.create_run(authority_context.clone()).unwrap();
        let authority_call = CallId::new("science_project_mutation");
        science_store
            .request_approval(Approval {
                project_id: authority_context.project_id.clone(),
                run_id: authority_context.run_id.clone(),
                call_id: authority_call.clone(),
                owner_id: authority_context.owner_id.clone(),
                decision: ApprovalDecision::Pending,
                decided_at: None,
            })
            .unwrap();
        science_store
            .transition(&authority_context.run_id, RunState::AwaitingApproval, None)
            .unwrap();
        science_store
            .decide_approval(
                &authority_context.project_id,
                &authority_context.run_id,
                &authority_context.owner_id,
                &authority_call,
                ApprovalDecision::Allow,
            )
            .unwrap();
        science_store
            .transition(&authority_context.run_id, RunState::Running, None)
            .unwrap();
        let bundle = admission
            .authorize_after_allow(&science_store, &authority_context)
            .unwrap();
        (admission, bundle)
    }

    fn copy_verified_migration_targets(
        root: &Path,
        request: &MutationRequest,
        bundle: &super::super::migration::VerifiedMigrationBundle,
    ) {
        let ProjectMutation::ProjectMigrate {
            authority_run_id, ..
        } = &request.mutation
        else {
            panic!("migration target copy fixture requires ProjectMigrate");
        };
        let science_store = ScienceStore::new(root);
        let target = request.migration_target_project_id().unwrap().unwrap();
        for (artifact, bytes) in bundle.artifacts() {
            science_store
                .put_artifact(
                    &ScienceProjectId::new(target.0.clone()),
                    &RunId::new(authority_run_id),
                    &request.owner_id,
                    CallId::new("science_project_mutation"),
                    &artifact.target_relative_path,
                    bytes,
                    artifact.mime.clone(),
                    artifact.preview.clone(),
                )
                .unwrap();
        }
    }

    #[test]
    fn compound_mutations_declare_every_required_feature() {
        use ScienceFeature::{
            ClaimLifecycle, Collaboration, EvidenceGraph, MigrationChain, ResearchProject,
            ReviewPackage,
        };

        assert_eq!(
            create_request("op-features-create")
                .mutation
                .required_features(),
            &[ResearchProject]
        );
        let migrate = ProjectMutation::ProjectMigrate {
            source_run_id: "run-1".into(),
            title: "Migrated".into(),
            research_question: "Question?".into(),
            authority_run_id: "authority-run-1".into(),
        };
        assert_eq!(
            migrate.required_features(),
            &[
                ResearchProject,
                MigrationChain,
                EvidenceGraph,
                ClaimLifecycle,
            ]
        );
        let evidence = ProjectMutation::EvidenceAttach {
            project_id: ProjectId("project-1".into()),
            claim_id: "claim-1".into(),
            artifact_sha256: "a".repeat(64),
            label: "artifact".into(),
            run_id: None,
        };
        assert_eq!(
            evidence.required_features(),
            &[ResearchProject, EvidenceGraph, ClaimLifecycle]
        );
        let review = ProjectMutation::ReviewRecord {
            project_id: ProjectId("project-1".into()),
            reviewer_id: "reviewer-1".into(),
            verdict: ReviewVerdict::Pass,
            summary: "Reviewed exact bytes.".into(),
            claim_id: None,
            source_run_id: "run-1".into(),
            authority_run_id: "authority-run-1".into(),
            artifact_sha256s: vec!["a".repeat(64)],
        };
        assert_eq!(
            review.required_features(),
            &[
                ResearchProject,
                EvidenceGraph,
                ClaimLifecycle,
                Collaboration,
                ReviewPackage,
            ]
        );
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
    fn migration_is_a_typed_idempotent_mutation() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let request = MutationRequest {
            operation_id: "op-migrate-0001".into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            expected_revision: None,
            mutation: ProjectMutation::ProjectMigrate {
                source_run_id: "v1-run-42".into(),
                title: "Migrated study".into(),
                research_question: "What survived migration?".into(),
                authority_run_id: "authority-run-42".into(),
            },
        };

        let direct = store.apply_mutation(&request).unwrap_err();
        assert!(
            matches!(&direct, ScienceError::Invalid(message)
                if message.contains("SessionActor-verified source bundle")),
            "bare ProjectStore migration was accepted: {direct}"
        );
        let (admission, bundle) = verified_migration(dir.path(), &request);
        let journal = store
            .admit_actor_migration(&request, &admission, &bundle)
            .unwrap();
        assert_eq!(journal.operation_id, request.operation_id);
        assert!(
            store.list_projects().unwrap().is_empty(),
            "journal admission published a project before target copying"
        );
        assert!(
            store
                .lookup_operation(&request.operation_id)
                .unwrap()
                .is_none(),
            "journal admission burned the generic operation ledger"
        );
        assert!(
            store
                .apply_actor_migration(&request, &admission, &bundle)
                .is_err(),
            "project records published before target-owned copies existed"
        );
        assert!(store.list_projects().unwrap().is_empty());
        copy_verified_migration_targets(dir.path(), &request, &bundle);
        let first = store
            .apply_actor_migration(&request, &admission, &bundle)
            .unwrap();
        let science_store = ScienceStore::new(dir.path());
        let authority_run_id = RunId::new("authority-run-42");
        assert!(
            science_store
                .transition(
                    &authority_run_id,
                    RunState::Cancelled,
                    Some("concurrent cancellation after project commit".into()),
                )
                .is_err(),
            "authority cancellation crossed the durable project commit fence"
        );
        assert_eq!(
            science_store.load_run(&authority_run_id).unwrap().state,
            RunState::Running
        );
        assert!(
            science_store
                .transition_succeeded_verified(&authority_run_id)
                .is_err(),
            "compatibility success bypassed the fenced exact completion manifest"
        );
        assert_eq!(first.kind, "project_migrate");
        let migration: super::super::migration::MigrationResult =
            serde_json::from_value(first.result.clone()).unwrap();
        assert_eq!(migration.source_run_id, "v1-run-42");
        assert_eq!(migration.target_project_id, first.project_id);
        let project = store.load_project(&first.project_id).unwrap();
        assert_eq!(project.owner_id.0, "owner-1");
        assert!(project.sessions.contains(&"v1-run-42".to_string()));

        let replay = store
            .apply_actor_migration(&request, &admission, &bundle)
            .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.project_id, first.project_id);
        assert_eq!(store.list_projects().unwrap().len(), 1);

        let result: super::super::migration::MigrationResult =
            serde_json::from_value(first.result).unwrap();
        store.verify_migration_result(&request, &result).unwrap();
        let recovery_grant = super::super::migration::MigrationRecoveryGrant::verify(
            &ScienceStore::new(dir.path()),
            &journal,
        )
        .unwrap();
        store
            .propose_claim(
                &replay.project_id,
                "owner-1",
                "A later researcher claim",
                "researcher",
            )
            .unwrap();
        let evolved_revision = store.project_revision(&replay.project_id).unwrap();
        store
            .apply_mutation(&MutationRequest {
                operation_id: "op-migrate-later-question".into(),
                session_id: request.session_id.clone(),
                owner_id: request.owner_id.clone(),
                expected_revision: Some(evolved_revision),
                mutation: ProjectMutation::QuestionUpdate {
                    project_id: replay.project_id.clone(),
                    research_question: "A legitimately refined research question".into(),
                },
            })
            .unwrap();
        assert_eq!(
            store
                .load_project(&replay.project_id)
                .unwrap()
                .research_question,
            "A legitimately refined research question"
        );
        store
            .verify_migration_result(&request, &result)
            .expect("later claim/question evolution must not invalidate migration-owned records");
        let recovered_replay = store
            .recover_actor_migration_operation(&request, &recovery_grant)
            .expect("later claim/question evolution must not invalidate authority-bound replay");
        assert!(recovered_replay.replayed);
        assert_eq!(recovered_replay.project_id, replay.project_id);
        assert_eq!(
            serde_json::from_value::<super::super::migration::MigrationResult>(
                recovered_replay.result
            )
            .unwrap(),
            result
        );

        let migration_claim_path = store
            .project_dir(&replay.project_id)
            .join("claims/migration-claim-0000.json");
        let mut migration_claim: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&migration_claim_path).unwrap()).unwrap();
        migration_claim["statement"] = serde_json::json!("tampered migration claim");
        std::fs::write(
            migration_claim_path,
            serde_json::to_vec_pretty(&migration_claim).unwrap(),
        )
        .unwrap();
        assert!(
            store.verify_migration_result(&request, &result).is_err(),
            "semantic migration-claim tamper replayed as valid"
        );
    }

    #[test]
    fn journal_only_migration_recovers_original_running_authority() {
        let dir = tempdir().unwrap();
        let project_store = ProjectStore::new(dir.path());
        let request = MutationRequest {
            operation_id: "op-migrate-journal-recovery".into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            expected_revision: None,
            mutation: ProjectMutation::ProjectMigrate {
                source_run_id: "source-journal-recovery".into(),
                title: "Recovered migration".into(),
                research_question: "Can the original authority finish?".into(),
                authority_run_id: "authority-journal-recovery".into(),
            },
        };
        let (admission, bundle) = verified_migration(dir.path(), &request);
        let commit = project_store
            .admit_actor_migration(&request, &admission, &bundle)
            .unwrap();
        assert!(project_store.list_projects().unwrap().is_empty());

        let science_store = ScienceStore::new(dir.path());
        let target_project = ScienceProjectId::new(commit.manifest.target_project_id.0.clone());
        let authority_run = commit.manifest.authority_run_id.clone();
        for (artifact, bytes) in bundle.artifacts() {
            science_store
                .put_artifact(
                    &target_project,
                    &authority_run,
                    &request.owner_id,
                    CallId::new("science_project_mutation"),
                    &artifact.target_relative_path,
                    bytes,
                    artifact.mime.clone(),
                    artifact.preview.clone(),
                )
                .unwrap();
        }
        let grant =
            super::super::migration::MigrationRecoveryGrant::verify(&science_store, &commit)
                .unwrap();
        let recovered = project_store
            .recover_actor_migration_operation(&request, &grant)
            .unwrap();
        assert!(recovered.replayed);
        assert_eq!(recovered.project_id, commit.manifest.target_project_id);
        assert!(project_store.load_project(&recovered.project_id).is_ok());
        assert!(
            project_store
                .lookup_operation(&request.operation_id)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn terminal_authority_cannot_publish_a_journal_only_project() {
        let dir = tempdir().unwrap();
        let project_store = ProjectStore::new(dir.path());
        let request = MutationRequest {
            operation_id: "op-migrate-terminal-recovery".into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            expected_revision: None,
            mutation: ProjectMutation::ProjectMigrate {
                source_run_id: "source-terminal-recovery".into(),
                title: "Terminal migration".into(),
                research_question: "Can terminal authority publish?".into(),
                authority_run_id: "authority-terminal-recovery".into(),
            },
        };
        let (admission, bundle) = verified_migration(dir.path(), &request);
        let commit = project_store
            .admit_actor_migration(&request, &admission, &bundle)
            .unwrap();
        let science_store = ScienceStore::new(dir.path());
        let target_project = ScienceProjectId::new(commit.manifest.target_project_id.0.clone());
        let authority_run = commit.manifest.authority_run_id.clone();
        for (artifact, bytes) in bundle.artifacts() {
            science_store
                .put_artifact(
                    &target_project,
                    &authority_run,
                    &request.owner_id,
                    CallId::new("science_project_mutation"),
                    &artifact.target_relative_path,
                    bytes,
                    artifact.mime.clone(),
                    artifact.preview.clone(),
                )
                .unwrap();
        }
        science_store
            .transition_succeeded_verified(&authority_run)
            .unwrap();
        let grant =
            super::super::migration::MigrationRecoveryGrant::verify(&science_store, &commit)
                .unwrap();
        let error = project_store
            .recover_actor_migration_operation(&request, &grant)
            .unwrap_err();
        assert!(
            error.to_string().contains("terminal migration authority"),
            "{error}"
        );
        assert!(project_store.list_projects().unwrap().is_empty());
        assert!(
            project_store
                .lookup_operation(&request.operation_id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn stale_bundle_cannot_admit_after_authority_terminalizes() {
        let dir = tempdir().unwrap();
        let project_store = ProjectStore::new(dir.path());
        let request = MutationRequest {
            operation_id: "op-migrate-stale-bundle".into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            expected_revision: None,
            mutation: ProjectMutation::ProjectMigrate {
                source_run_id: "source-stale-bundle".into(),
                title: "Stale bundle".into(),
                research_question: "Can a terminalized grant write?".into(),
                authority_run_id: "authority-stale-bundle".into(),
            },
        };
        let (admission, bundle) = verified_migration(dir.path(), &request);
        ScienceStore::new(dir.path())
            .transition_succeeded_verified(&RunId::new("authority-stale-bundle"))
            .unwrap();
        let error = project_store
            .admit_actor_migration(&request, &admission, &bundle)
            .unwrap_err();
        assert!(
            error.to_string().contains("exact Running authority run"),
            "{error}"
        );
        assert!(
            project_store
                .lookup_migration_commit(&request.operation_id)
                .unwrap()
                .is_none()
        );
        assert!(project_store.list_projects().unwrap().is_empty());
    }

    #[test]
    fn migration_rejects_cross_root_project_store() {
        let authority_root = tempdir().unwrap();
        let project_root = tempdir().unwrap();
        let request = MutationRequest {
            operation_id: "op-migrate-cross-root".into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            expected_revision: None,
            mutation: ProjectMutation::ProjectMigrate {
                source_run_id: "source-cross-root".into(),
                title: "Cross-root migration".into(),
                research_question: "Can another project store consume this authority?".into(),
                authority_run_id: "authority-cross-root".into(),
            },
        };
        let (admission, bundle) = verified_migration(authority_root.path(), &request);
        let project_store = ProjectStore::new(project_root.path());
        let error = project_store
            .admit_actor_migration(&request, &admission, &bundle)
            .unwrap_err();
        assert!(
            error.to_string().contains("different roots"),
            "cross-root migration failed for the wrong reason: {error}"
        );
        assert!(
            project_store
                .lookup_migration_commit(&request.operation_id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn migration_journal_and_recovery_reject_expected_revision() {
        let dir = tempdir().unwrap();
        let project_store = ProjectStore::new(dir.path());
        let valid = MutationRequest {
            operation_id: "op-migrate-unexpected-revision".into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            expected_revision: None,
            mutation: ProjectMutation::ProjectMigrate {
                source_run_id: "source-unexpected-revision".into(),
                title: "No target revision".into(),
                research_question: "Can create-only migration carry a CAS revision?".into(),
                authority_run_id: "authority-unexpected-revision".into(),
            },
        };
        let (admission, bundle) = verified_migration(dir.path(), &valid);
        let mut forged = valid.clone();
        forged.expected_revision = Some("forged-revision".into());
        assert!(
            project_store
                .admit_actor_migration(&forged, &admission, &bundle)
                .is_err(),
            "migration journal accepted expectedRevision"
        );
        assert!(
            project_store
                .lookup_migration_commit(&valid.operation_id)
                .unwrap()
                .is_none()
        );

        let commit = project_store
            .admit_actor_migration(&valid, &admission, &bundle)
            .unwrap();
        copy_verified_migration_targets(dir.path(), &valid, &bundle);
        let grant = super::super::migration::MigrationRecoveryGrant::verify(
            &ScienceStore::new(dir.path()),
            &commit,
        )
        .unwrap();
        assert!(
            project_store
                .recover_actor_migration_operation(&forged, &grant)
                .is_err(),
            "migration recovery accepted expectedRevision"
        );
        assert!(project_store.list_projects().unwrap().is_empty());
        assert!(
            project_store
                .lookup_operation(&valid.operation_id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn migration_recovery_revalidates_grant_after_mint_before_publish() {
        let dir = tempdir().unwrap();
        let project_store = ProjectStore::new(dir.path());
        let request = MutationRequest {
            operation_id: "op-migrate-stale-recovery-grant".into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            expected_revision: None,
            mutation: ProjectMutation::ProjectMigrate {
                source_run_id: "source-stale-recovery-grant".into(),
                title: "Stale recovery grant".into(),
                research_question: "Must publish revalidate authority state?".into(),
                authority_run_id: "authority-stale-recovery-grant".into(),
            },
        };
        let (admission, bundle) = verified_migration(dir.path(), &request);
        let commit = project_store
            .admit_actor_migration(&request, &admission, &bundle)
            .unwrap();
        copy_verified_migration_targets(dir.path(), &request, &bundle);
        let science_store = ScienceStore::new(dir.path());
        let grant =
            super::super::migration::MigrationRecoveryGrant::verify(&science_store, &commit)
                .unwrap();
        science_store
            .transition_succeeded_verified(&commit.manifest.authority_run_id)
            .unwrap();
        let error = project_store
            .recover_actor_migration_operation(&request, &grant)
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("terminal migration authority may verify replay"),
            "stale recovery grant failed for the wrong reason: {error}"
        );
        assert!(project_store.list_projects().unwrap().is_empty());
        assert!(
            project_store
                .lookup_operation(&request.operation_id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn migration_replay_rejects_another_live_bundle() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let original = MutationRequest {
            operation_id: "op-migrate-replay-original".into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            expected_revision: None,
            mutation: ProjectMutation::ProjectMigrate {
                source_run_id: "source-replay-original".into(),
                title: "Original migration".into(),
                research_question: "Which exact source was allowed?".into(),
                authority_run_id: "authority-replay-original".into(),
            },
        };
        let (original_admission, original_bundle) = verified_migration(dir.path(), &original);
        copy_verified_migration_targets(dir.path(), &original, &original_bundle);
        store
            .apply_actor_migration(&original, &original_admission, &original_bundle)
            .unwrap();

        let unrelated = MutationRequest {
            operation_id: "op-migrate-replay-unrelated".into(),
            mutation: ProjectMutation::ProjectMigrate {
                source_run_id: "source-replay-unrelated".into(),
                title: "Unrelated migration".into(),
                research_question: "Can its Allow retrieve another result?".into(),
                authority_run_id: "authority-replay-unrelated".into(),
            },
            ..original.clone()
        };
        let (_unrelated_admission, unrelated_bundle) = verified_migration(dir.path(), &unrelated);
        let error = store
            .apply_actor_migration(&original, &original_admission, &unrelated_bundle)
            .unwrap_err();
        assert!(
            matches!(error, ScienceError::Ownership),
            "another live bundle reached the existing-operation replay fast path: {error}"
        );
    }

    #[test]
    fn migration_recovery_grant_rejects_replaced_same_request_commit() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let request = MutationRequest {
            operation_id: "op-migrate-replaced-commit".into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            expected_revision: None,
            mutation: ProjectMutation::ProjectMigrate {
                source_run_id: "source-replaced-commit".into(),
                title: "Retained commit".into(),
                research_question: "Can a journal be substituted after grant mint?".into(),
                authority_run_id: "authority-replaced-commit".into(),
            },
        };
        let (admission, bundle) = verified_migration(dir.path(), &request);
        let journal = store
            .admit_actor_migration(&request, &admission, &bundle)
            .unwrap();
        copy_verified_migration_targets(dir.path(), &request, &bundle);
        let grant = super::super::migration::MigrationRecoveryGrant::verify(
            &ScienceStore::new(dir.path()),
            &journal,
        )
        .unwrap();

        let replacement_manifest = bundle
            .manifest(journal.manifest.generated_at + chrono::Duration::seconds(1))
            .unwrap();
        let replacement = super::super::migration::MigrationCommit::new(
            journal.request_sha256.clone(),
            admission,
            replacement_manifest,
        )
        .unwrap();
        assert_ne!(replacement, journal);
        std::fs::write(
            dir.path()
                .join("migration-commits")
                .join(format!("{}.json", request.operation_id)),
            serde_json::to_vec_pretty(&replacement).unwrap(),
        )
        .unwrap();

        let error = store
            .recover_actor_migration_operation(&request, &grant)
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("differs from the grant's retained commit"),
            "replacement journal was not rejected by exact grant binding: {error}"
        );
        assert!(store.list_projects().unwrap().is_empty());
        assert!(
            store
                .lookup_operation(&request.operation_id)
                .unwrap()
                .is_none()
        );
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
        let created = store
            .apply_mutation(&create_request("op-create-0003"))
            .unwrap();

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
    fn operation_id_cannot_replay_changed_payload_of_the_same_kind() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let original = create_request("op-create-payload");
        let first = store.apply_mutation(&original).unwrap();

        let mut changed = original;
        changed.mutation = ProjectMutation::ProjectCreate {
            title: "A different project".into(),
            research_question: "This was never approved.".into(),
        };
        let error = store.apply_mutation(&changed).unwrap_err();
        assert!(
            matches!(&error, ScienceError::Invalid(message)
                if message.contains("does not match its original request")),
            "unexpected: {error}"
        );
        assert_eq!(store.list_projects().unwrap().len(), 1);
        assert_eq!(store.load_project(&first.project_id).unwrap().title, "Demo");
    }

    #[test]
    fn operation_id_cannot_replay_the_same_kind_against_another_project() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let project_a = store
            .apply_mutation(&create_request("op-create-target-a"))
            .unwrap();
        let project_b = store
            .apply_mutation(&create_request("op-create-target-b"))
            .unwrap();
        let update_a = MutationRequest {
            operation_id: "op-question-target".into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            expected_revision: None,
            mutation: ProjectMutation::QuestionUpdate {
                project_id: project_a.project_id.clone(),
                research_question: "Approved question A".into(),
            },
        };
        store.apply_mutation(&update_a).unwrap();

        let update_b = MutationRequest {
            mutation: ProjectMutation::QuestionUpdate {
                project_id: project_b.project_id.clone(),
                research_question: "Unapproved question B".into(),
            },
            ..update_a
        };
        let error = store.apply_mutation(&update_b).unwrap_err();
        assert!(
            matches!(&error, ScienceError::Invalid(message)
                if message.contains("does not match its original request")),
            "unexpected: {error}"
        );
        assert_eq!(
            store
                .load_project(&project_b.project_id)
                .unwrap()
                .research_question,
            "Does EcoRI cut?"
        );
    }

    #[test]
    fn operation_id_cannot_replay_with_a_different_revision_precondition() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let created = store
            .apply_mutation(&create_request("op-create-revision"))
            .unwrap();
        let original = MutationRequest {
            operation_id: "op-question-revision".into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            expected_revision: Some(created.revision),
            mutation: ProjectMutation::QuestionUpdate {
                project_id: created.project_id,
                research_question: "Approved against one exact revision".into(),
            },
        };
        let first = store.apply_mutation(&original).unwrap();

        let mut changed = original.clone();
        changed.expected_revision = None;
        let error = store.apply_mutation(&changed).unwrap_err();
        assert!(
            matches!(&error, ScienceError::Invalid(message)
                if message.contains("does not match its original request")),
            "unexpected: {error}"
        );

        let replay = store.apply_mutation(&original).unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.revision, first.revision);
    }

    #[test]
    fn legacy_operation_without_a_request_digest_fails_closed() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let request = create_request("op-legacy-digest");
        store.apply_mutation(&request).unwrap();
        let record = store
            .lookup_operation(&request.operation_id)
            .unwrap()
            .expect("operation record");
        let mut encoded = serde_json::to_value(record).unwrap();
        encoded
            .as_object_mut()
            .expect("record object")
            .remove("request_sha256");
        let legacy: OperationRecord = serde_json::from_value(encoded).unwrap();

        assert!(legacy.request_sha256.is_empty());
        assert!(
            matches!(
                legacy.verify_replay(&request),
                Err(ScienceError::Invalid(message))
                    if message.contains("predates request-bound replay")
            ),
            "legacy operation replay did not fail closed"
        );
    }

    #[test]
    fn migration_operation_id_cannot_replay_changed_source_or_metadata() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let original = MutationRequest {
            operation_id: "op-migrate-binding".into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            expected_revision: None,
            mutation: ProjectMutation::ProjectMigrate {
                source_run_id: "source-run-a".into(),
                title: "Approved migration".into(),
                research_question: "What survived?".into(),
                authority_run_id: "authority-run-a".into(),
            },
        };
        let (admission, bundle) = verified_migration(dir.path(), &original);
        copy_verified_migration_targets(dir.path(), &original, &bundle);
        store
            .apply_actor_migration(&original, &admission, &bundle)
            .unwrap();
        let changed = MutationRequest {
            mutation: ProjectMutation::ProjectMigrate {
                source_run_id: "source-run-b".into(),
                title: "Unapproved migration".into(),
                research_question: "Different payload".into(),
                authority_run_id: "authority-run-b".into(),
            },
            ..original
        };
        assert!(
            matches!(
                store.apply_actor_migration(&changed, &admission, &bundle),
                Err(ScienceError::Ownership)
            ),
            "changed migration payload replayed"
        );
        assert_eq!(store.list_projects().unwrap().len(), 1);
    }

    #[test]
    fn expected_revision_rejects_a_stale_mutation() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let created = store
            .apply_mutation(&create_request("op-create-0004"))
            .unwrap();
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
        let created = store
            .apply_mutation(&create_request("op-create-0006"))
            .unwrap();
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
        let created = store
            .apply_mutation(&create_request("op-create-0005"))
            .unwrap();

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
        let created = store
            .apply_mutation(&create_request("op-chain-00001"))
            .unwrap();
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
        let created = store
            .apply_mutation(&create_request("op-rev-000001"))
            .unwrap();
        let project_id = created.project_id.clone();

        let mut seen = std::collections::BTreeSet::new();
        seen.insert(created.revision.clone());
        assert_eq!(
            store.project_revision(&project_id).unwrap(),
            created.revision
        );

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
