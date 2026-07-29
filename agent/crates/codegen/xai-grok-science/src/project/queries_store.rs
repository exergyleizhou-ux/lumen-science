//! WP-3 product path: evidence queries + migration via ProjectStore.
use super::model::ProjectId;
use super::query::{ClaimComparison, ConsistencyReport, EvidenceTrace, ReproductionStatus};
use super::store::ProjectStore;
use super::{
    Claim, ClaimStatus, EdgeKind, EvidenceEdge, EvidenceGraph, EvidenceNode, NodeId, NodeKind,
    RegisteredArtifact,
};
use chrono::Utc;
use std::collections::BTreeMap;

impl ProjectStore {
    /// Trace evidence from a claim back to source artifacts.
    pub fn trace_evidence(
        &self,
        project_id: &ProjectId,
        claim_id: &str,
    ) -> crate::Result<EvidenceTrace> {
        self.gates()
            .require(crate::features::ScienceFeature::EvidenceQuery)?;
        let claim = self.load_claim(project_id, claim_id)?;
        let nid = claim
            .evidence_node_id
            .as_ref()
            .ok_or_else(|| crate::ScienceError::Invalid("claim has no evidence node".into()))?;
        let graph = self.load_graph(project_id)?;
        graph
            .trace_evidence(nid)
            .map_err(crate::ScienceError::Invalid)
    }

    pub fn compare_claims(
        &self,
        project_id: &ProjectId,
        claim_a: &str,
        claim_b: &str,
    ) -> crate::Result<ClaimComparison> {
        self.gates()
            .require(crate::features::ScienceFeature::EvidenceQuery)?;
        let a = self
            .load_claim(project_id, claim_a)?
            .evidence_node_id
            .ok_or_else(|| crate::ScienceError::Invalid("claim A missing evidence node".into()))?;
        let b = self
            .load_claim(project_id, claim_b)?
            .evidence_node_id
            .ok_or_else(|| crate::ScienceError::Invalid("claim B missing evidence node".into()))?;
        let graph = self.load_graph(project_id)?;
        graph
            .compare_claims(&a, &b)
            .map_err(crate::ScienceError::Invalid)
    }

    pub fn check_consistency(&self, project_id: &ProjectId) -> crate::Result<ConsistencyReport> {
        self.gates()
            .require(crate::features::ScienceFeature::EvidenceQuery)?;
        let graph = self.load_graph(project_id)?;
        Ok(graph.check_consistency())
    }

    pub fn reproduction_status(
        &self,
        project_id: &ProjectId,
        claim_id: &str,
    ) -> crate::Result<ReproductionStatus> {
        self.gates()
            .require(crate::features::ScienceFeature::ReproductionReport)?;
        let claim = self.load_claim(project_id, claim_id)?;
        let nid = claim
            .evidence_node_id
            .as_ref()
            .ok_or_else(|| crate::ScienceError::Invalid("claim has no evidence node".into()))?;
        let graph = self.load_graph(project_id)?;
        graph
            .reproduction_status(nid)
            .map_err(crate::ScienceError::Invalid)
    }

    /// Commit a byte-verified source bundle into one deterministic V2 project.
    ///
    /// Caller must hold the project-store write guard and can obtain
    /// `VerifiedMigrationBundle` only by revalidating the actor's immutable
    /// admission after durable Allow.
    pub(super) fn migrate_v1_to_v2_inner(
        &self,
        request: &super::mutation::MutationRequest,
        admission: &super::migration::MigrationAdmission,
        source: &super::migration::VerifiedMigrationBundle,
    ) -> crate::Result<super::migration::MigrationResult> {
        let commit = self.admit_migration_commit_inner(request, admission, source)?;
        source.verify_target_copies_for_project_store(self)?;
        publish_migration_commit(self, request, &commit)?;
        super::migration::MigrationResult::from_commit(&commit)
    }

    /// Persist the immutable operation/source/authority journal before the
    /// actor copies even the first target artifact. The caller holds the
    /// project-store write guard and can supply `source` only after the
    /// authority run has a durable Allow.
    pub(super) fn admit_migration_commit_inner(
        &self,
        request: &super::mutation::MutationRequest,
        admission: &super::migration::MigrationAdmission,
        source: &super::migration::VerifiedMigrationBundle,
    ) -> crate::Result<super::migration::MigrationCommit> {
        use crate::ScienceError;

        self.gates()
            .require(crate::features::ScienceFeature::MigrationChain)?;
        self.gates()
            .require(crate::features::ScienceFeature::EvidenceGraph)?;
        self.gates()
            .require(crate::features::ScienceFeature::ClaimLifecycle)?;
        source.verify_live_authority_for_project_store(self)?;
        let super::mutation::ProjectMutation::ProjectMigrate {
            source_run_id,
            title,
            research_question,
            authority_run_id,
        } = &request.mutation
        else {
            return Err(ScienceError::Invalid(
                "verified migration bundle used for a non-migration request".into(),
            ));
        };
        let target_project_id = request.migration_target_project_id()?.ok_or_else(|| {
            ScienceError::Invalid("migration request has no deterministic target".into())
        })?;
        if admission.operation_id() != request.operation_id
            || admission.source_run_id().0 != *source_run_id
            || admission.target_project_id() != &target_project_id
            || admission.authority_run_id().0 != *authority_run_id
            || admission.owner_id() != request.owner_id
            || admission.session_id() != request.session_id
            || admission.title() != title
            || admission.research_question() != research_question
            || source.admission()? != admission
            || source.source_run().context.run_id != *admission.source_run_id()
            || source.snapshot() != admission.source_snapshot()
        {
            return Err(ScienceError::Ownership);
        }

        let request_sha256 = request.replay_fingerprint()?;
        let commit = match self.lookup_migration_commit(&request.operation_id)? {
            Some(commit) => {
                commit.verify()?;
                if commit.request_sha256 != request_sha256
                    || commit.admission != *admission
                    || commit.manifest.target_project_id != target_project_id
                {
                    return Err(ScienceError::Invalid(format!(
                        "migration commit {} does not match its admitted request",
                        request.operation_id
                    )));
                }
                commit
            }
            None => {
                let manifest = super::migration::MigrationManifest::from_verified(
                    admission,
                    source,
                    Utc::now(),
                )?;
                let commit = super::migration::MigrationCommit::new(
                    request_sha256.clone(),
                    admission.clone(),
                    manifest,
                )?;
                // Journal first. Any later interruption can only recover this
                // exact target/source/authority tuple.
                self.write_migration_commit_inner(&commit)?;
                commit
            }
        };

        Ok(commit)
    }

    /// Resume the project-side half of a journaled migration. The caller must
    /// already hold the write guard and prove the original authority run via a
    /// `MigrationRecoveryGrant`.
    pub(super) fn resume_migration_commit_inner(
        &self,
        request: &super::mutation::MutationRequest,
        commit: &super::migration::MigrationCommit,
    ) -> crate::Result<super::migration::MigrationResult> {
        publish_migration_commit(self, request, commit)?;
        super::migration::MigrationResult::from_commit(commit)
    }

    /// Reopen every project-side record behind a completed or recoverable
    /// migration. SessionActor combines this with ScienceStore byte and
    /// approval verification before replaying an outcome.
    pub fn verify_migration_result(
        &self,
        request: &super::mutation::MutationRequest,
        result: &super::migration::MigrationResult,
    ) -> crate::Result<super::migration::MigrationCommit> {
        if !matches!(
            request.mutation,
            super::mutation::ProjectMutation::ProjectMigrate { .. }
        ) {
            return Err(crate::ScienceError::Invalid(
                "migration verification requires a project_migrate request".into(),
            ));
        }
        let commit = self
            .lookup_migration_commit(&request.operation_id)?
            .ok_or_else(|| {
                crate::ScienceError::Invalid(format!(
                    "migration commit {} is missing",
                    request.operation_id
                ))
            })?;
        commit.verify()?;
        if commit.request_sha256 != request.replay_fingerprint()?
            || super::migration::MigrationResult::from_commit(&commit)? != *result
        {
            return Err(crate::ScienceError::Invalid(
                "migration result does not match its durable commit".into(),
            ));
        }
        verify_committed_migration(self, request, &commit)?;
        Ok(commit)
    }
}

fn publish_migration_commit(
    store: &ProjectStore,
    request: &super::mutation::MutationRequest,
    commit: &super::migration::MigrationCommit,
) -> crate::Result<()> {
    use crate::ScienceError;

    commit.verify()?;
    let target_project_id = commit.manifest.target_project_id.clone();
    if store.project_exists(&target_project_id)? {
        return verify_committed_migration(store, request, commit);
    }
    let super::mutation::ProjectMutation::ProjectMigrate {
        source_run_id,
        title,
        research_question,
        authority_run_id,
    } = &request.mutation
    else {
        return Err(ScienceError::Invalid(
            "migration commit used for a non-migration request".into(),
        ));
    };
    if source_run_id != &commit.manifest.source_run.context.run_id.0
        || authority_run_id != &commit.manifest.authority_run_id.0
        || title != &commit.manifest.title
        || research_question != &commit.manifest.research_question
        || request.replay_fingerprint()? != commit.request_sha256
    {
        return Err(ScienceError::Ownership);
    }
    let generated_at = commit.manifest.generated_at;
    let mut project = super::migration::V1ToV2Migration::create_project_from_run(
        target_project_id.clone(),
        super::model::OwnerId(request.owner_id.clone()),
        title,
        research_question,
        vec![source_run_id.clone(), authority_run_id.clone()],
    );
    project.evidence_graph_id = Some(format!("graph-{}", target_project_id.0));
    project.created_at = generated_at;
    project.updated_at = generated_at;

    let mut registry = BTreeMap::new();
    for artifact in &commit.manifest.artifacts {
        let record = RegisteredArtifact {
            project_id: target_project_id.clone(),
            sha256: artifact.sha256.clone(),
            label: artifact.target_relative_path.display().to_string(),
            run_id: Some(authority_run_id.clone()),
            registered_by: request.owner_id.clone(),
            registered_at: generated_at,
        };
        if registry.insert(record.sha256.clone(), record).is_some() {
            return Err(ScienceError::Invalid(
                "migration source contains duplicate artifact digests".into(),
            ));
        }
    }

    let mut graph = EvidenceGraph::new(target_project_id.clone());
    let mut claims = Vec::new();
    for (index, evidence) in commit.manifest.evidence.iter().enumerate() {
        let Some(sha256) = evidence.artifact_sha256.as_ref() else {
            continue;
        };
        if !registry.contains_key(sha256) {
            return Err(ScienceError::Invalid(
                "migration evidence cites an unregistered target artifact".into(),
            ));
        }
        let claim_id = format!("migration-claim-{index:04}");
        let claim_node_id = NodeId(format!("node-{claim_id}"));
        let mut claim = Claim::new(
            claim_id,
            target_project_id.clone(),
            evidence.claim.clone(),
            request.owner_id.clone(),
        );
        claim.status = ClaimStatus::EvidenceAttached;
        claim.evidence_node_id = Some(claim_node_id.clone());
        claim.limitations = vec![format!(
            "Migrated from source evidence record: {}",
            evidence.source
        )];
        claim.created_at = evidence.verified_at;
        claim.updated_at = evidence.verified_at;
        graph
            .add_node(EvidenceNode {
                node_id: claim_node_id.clone(),
                kind: NodeKind::Claim,
                project_id: target_project_id.clone(),
                label: evidence.claim.clone(),
                artifact_sha256: None,
                run_id: Some(authority_run_id.clone()),
                created_by: request.owner_id.clone(),
                created_at: evidence.verified_at,
                metadata: BTreeMap::from([
                    ("source_run_id".into(), source_run_id.clone()),
                    ("source".into(), evidence.source.clone()),
                ]),
            })
            .map_err(ScienceError::Invalid)?;
        let artifact_node_id = ProjectStore::artifact_node_id(sha256);
        if !graph.nodes.contains_key(&artifact_node_id) {
            graph
                .add_node(EvidenceNode {
                    node_id: artifact_node_id.clone(),
                    kind: NodeKind::SourceArtifact,
                    project_id: target_project_id.clone(),
                    label: registry
                        .get(sha256)
                        .expect("registry checked above")
                        .label
                        .clone(),
                    artifact_sha256: Some(sha256.clone()),
                    run_id: Some(authority_run_id.clone()),
                    created_by: request.owner_id.clone(),
                    created_at: evidence.verified_at,
                    metadata: BTreeMap::from([("source_run_id".into(), source_run_id.clone())]),
                })
                .map_err(ScienceError::Invalid)?;
        }
        graph
            .add_edge(EvidenceEdge {
                source: artifact_node_id,
                target: claim_node_id,
                relation: EdgeKind::Supports,
                actor: request.owner_id.clone(),
                timestamp: evidence.verified_at,
                run_id: authority_run_id.clone(),
                supporting_artifact_sha256: sha256.clone(),
                confidence_kind: "high".into(),
            })
            .map_err(ScienceError::Invalid)?;
        claims.push(claim);
    }
    if claims.is_empty() {
        return Err(ScienceError::Invalid(
            "migration produced no artifact-bound evidence graph entries".into(),
        ));
    }

    // Publish every derivable child before project.json. The project file
    // is the commit marker read by list/load APIs.
    for claim in &claims {
        store.write_claim_file(claim)?;
    }
    store.write_graph_file(&graph)?;
    store.write_artifact_registry_inner(&target_project_id, &registry)?;
    store.write_migration_manifest_inner(&target_project_id, &commit.manifest)?;
    store.save_project_inner(&project)?;
    verify_committed_migration(store, request, commit)
}

pub(super) fn verify_committed_migration(
    store: &ProjectStore,
    request: &super::mutation::MutationRequest,
    commit: &super::migration::MigrationCommit,
) -> crate::Result<()> {
    use crate::ScienceError;

    commit.verify()?;
    let target = &commit.manifest.target_project_id;
    let project = store.load_project(target)?;
    if project.project_id != *target
        || project.owner_id.0 != request.owner_id
        || project.title != commit.manifest.title
        || !project
            .sessions
            .contains(&commit.manifest.source_run.context.run_id.0)
        || !project
            .sessions
            .contains(&commit.manifest.authority_run_id.0)
    {
        return Err(ScienceError::Ownership);
    }
    if store.load_migration_manifest(target)? != commit.manifest {
        return Err(ScienceError::Invalid(
            "published migration manifest differs from its commit journal".into(),
        ));
    }
    let registry = store.list_artifacts(target)?;
    for artifact in &commit.manifest.artifacts {
        let expected = RegisteredArtifact {
            project_id: target.clone(),
            sha256: artifact.sha256.clone(),
            label: artifact.target_relative_path.display().to_string(),
            run_id: Some(commit.manifest.authority_run_id.0.clone()),
            registered_by: request.owner_id.clone(),
            registered_at: commit.manifest.generated_at,
        };
        if registry.get(&artifact.sha256) != Some(&expected) {
            return Err(ScienceError::Invalid(
                "published migration artifact registry does not match its manifest".into(),
            ));
        }
    }
    let graph = store.load_graph(target)?;
    graph
        .validate_integrity()
        .map_err(|error| ScienceError::Invalid(format!("migration graph invalid: {error}")))?;
    for (index, evidence) in commit.manifest.evidence.iter().enumerate() {
        let Some(sha256) = evidence.artifact_sha256.as_ref() else {
            continue;
        };
        let record = registry.get(sha256).ok_or_else(|| {
            ScienceError::Invalid("migration graph cites an unregistered artifact".into())
        })?;
        let claim_id = format!("migration-claim-{index:04}");
        let claim_node_id = NodeId(format!("node-{claim_id}"));
        let mut expected_claim = Claim::new(
            claim_id.clone(),
            target.clone(),
            evidence.claim.clone(),
            request.owner_id.clone(),
        );
        expected_claim.status = ClaimStatus::EvidenceAttached;
        expected_claim.evidence_node_id = Some(claim_node_id.clone());
        expected_claim.limitations = vec![format!(
            "Migrated from source evidence record: {}",
            evidence.source
        )];
        expected_claim.created_at = evidence.verified_at;
        expected_claim.updated_at = evidence.verified_at;
        if serde_json::to_value(store.load_claim(target, &claim_id)?)?
            != serde_json::to_value(&expected_claim)?
        {
            return Err(ScienceError::Invalid(
                "published migration claim differs from its manifest".into(),
            ));
        }
        let expected_claim_node = EvidenceNode {
            node_id: claim_node_id.clone(),
            kind: NodeKind::Claim,
            project_id: target.clone(),
            label: evidence.claim.clone(),
            artifact_sha256: None,
            run_id: Some(commit.manifest.authority_run_id.0.clone()),
            created_by: request.owner_id.clone(),
            created_at: evidence.verified_at,
            metadata: BTreeMap::from([
                (
                    "source_run_id".into(),
                    commit.manifest.source_run.context.run_id.0.clone(),
                ),
                ("source".into(), evidence.source.clone()),
            ]),
        };
        if graph
            .nodes
            .get(&claim_node_id)
            .map(serde_json::to_value)
            .transpose()?
            != Some(serde_json::to_value(&expected_claim_node)?)
        {
            return Err(ScienceError::Invalid(
                "published migration claim node differs from its manifest".into(),
            ));
        }
        let artifact_node_id = ProjectStore::artifact_node_id(sha256);
        let first_artifact_evidence = commit
            .manifest
            .evidence
            .iter()
            .find(|candidate| candidate.artifact_sha256.as_ref() == Some(sha256))
            .expect("current evidence proves at least one artifact record");
        let expected_artifact_node = EvidenceNode {
            node_id: artifact_node_id.clone(),
            kind: NodeKind::SourceArtifact,
            project_id: target.clone(),
            label: record.label.clone(),
            artifact_sha256: Some(sha256.clone()),
            run_id: Some(commit.manifest.authority_run_id.0.clone()),
            created_by: request.owner_id.clone(),
            created_at: first_artifact_evidence.verified_at,
            metadata: BTreeMap::from([(
                "source_run_id".into(),
                commit.manifest.source_run.context.run_id.0.clone(),
            )]),
        };
        if graph
            .nodes
            .get(&artifact_node_id)
            .map(serde_json::to_value)
            .transpose()?
            != Some(serde_json::to_value(&expected_artifact_node)?)
        {
            return Err(ScienceError::Invalid(
                "published migration artifact node differs from its manifest".into(),
            ));
        }
        let expected_edge = EvidenceEdge {
            source: artifact_node_id,
            target: claim_node_id,
            relation: EdgeKind::Supports,
            actor: request.owner_id.clone(),
            timestamp: evidence.verified_at,
            run_id: commit.manifest.authority_run_id.0.clone(),
            supporting_artifact_sha256: sha256.clone(),
            confidence_kind: "high".into(),
        };
        let expected_edge = serde_json::to_value(expected_edge)?;
        if !graph
            .edges
            .iter()
            .map(serde_json::to_value)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .contains(&expected_edge)
        {
            return Err(ScienceError::Invalid(
                "published migration evidence edge differs from its manifest".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn trace_and_compare() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.create_project("o", "T", "Q").unwrap();
        let claim = store
            .propose_claim(&p.project_id, "o", "EcoRI cuts", "sci")
            .unwrap();
        let sha = "a".repeat(64);
        // Evidence may only cite a registered artifact.
        store
            .register_artifact(&p.project_id, "o", sha.clone(), "seq", Some("r1".into()))
            .unwrap();
        store
            .attach_evidence(
                &p.project_id,
                "o",
                &claim.claim_id,
                sha,
                "seq",
                Some("r1".into()),
            )
            .unwrap();
        let trace = store
            .trace_evidence(&p.project_id, &claim.claim_id)
            .unwrap();
        assert!(!trace.trace_steps.is_empty());

        let claim2 = store
            .propose_claim(&p.project_id, "o", "NotI also", "sci")
            .unwrap();
        store
            .register_artifact(
                &p.project_id,
                "o",
                "b".repeat(64),
                "seq2",
                Some("r2".into()),
            )
            .unwrap();
        store
            .attach_evidence(
                &p.project_id,
                "o",
                &claim2.claim_id,
                "b".repeat(64),
                "seq2",
                Some("r2".into()),
            )
            .unwrap();
        let cmp = store
            .compare_claims(&p.project_id, &claim.claim_id, &claim2.claim_id)
            .unwrap();
        assert!(
            !cmp.supports_same_conclusion
                || cmp.shared_evidence.len() + cmp.conflicting_evidence.len() > 0
        );

        let consistency = store.check_consistency(&p.project_id).unwrap();
        assert!(consistency.is_consistent);

        // Bare ProjectStore migration was intentionally removed. A migration
        // needs an actor-owned ScienceStore admission and verified bytes.
    }
}
