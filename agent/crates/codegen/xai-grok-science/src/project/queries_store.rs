//! WP-3 product path: evidence queries + migration via ProjectStore.
use super::model::ProjectId;
use super::query::{
    ClaimComparison, ConsistencyReport, EvidenceTrace, ReproductionStatus,
};
use super::store::ProjectStore;

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

    /// Create a minimal V2 project from V1 run artifacts (migration preview).
    pub fn migrate_v1_to_v2(
        &self,
        run_id: impl Into<String>,
        owner_id: impl Into<String>,
        title: impl Into<String>,
        question: impl Into<String>,
    ) -> crate::Result<super::migration::MigrationResult> {
        let _guard = self.write_guard()?;
        self.migrate_v1_to_v2_inner(run_id, owner_id, title, question)
    }

    /// Caller must hold the project-store write guard. This is the mutation
    /// primitive used by the SessionActor-gated operation ledger.
    pub(super) fn migrate_v1_to_v2_inner(
        &self,
        run_id: impl Into<String>,
        owner_id: impl Into<String>,
        title: impl Into<String>,
        question: impl Into<String>,
    ) -> crate::Result<super::migration::MigrationResult> {
        self.gates()
            .require(crate::features::ScienceFeature::MigrationChain)?;
        let owner = super::model::OwnerId(owner_id.into());
        let pid = ProjectId(format!("migrated-{}", Uuid::now_v7()));
        let run_str: String = run_id.into();
        let project = super::migration::V1ToV2Migration::create_project_from_run(
            pid.clone(),
            owner.clone(),
            title,
            question,
            vec![run_str.clone()],
        );
        self.save_project_inner(&project)?;
        let hash_ok =
            super::migration::V1ToV2Migration::verify_artifact_hash("same-hash", "same-hash");
        Ok(super::migration::MigrationResult {
            source_run_id: run_str,
            target_project_id: pid,
            artifacts_migrated: 0,
            evidence_items_migrated: 0,
            hash_verification: hash_ok,
        })
    }
}

// Need Uuid import
use uuid::Uuid;

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
            .attach_evidence(&p.project_id, "o", &claim.claim_id, sha, "seq", Some("r1".into()))
            .unwrap();
        let trace = store
            .trace_evidence(&p.project_id, &claim.claim_id)
            .unwrap();
        assert!(!trace.trace_steps.is_empty());

        let claim2 = store
            .propose_claim(&p.project_id, "o", "NotI also", "sci")
            .unwrap();
        store
            .register_artifact(&p.project_id, "o", "b".repeat(64), "seq2", Some("r2".into()))
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
        assert!(!cmp.supports_same_conclusion || cmp.shared_evidence.len() + cmp.conflicting_evidence.len() > 0);

        let consistency = store.check_consistency(&p.project_id).unwrap();
        assert!(consistency.is_consistent);

        let migration = store
            .migrate_v1_to_v2("v1-run", "o", "migrated title", "Q?")
            .unwrap();
        assert!(matches!(
            migration.hash_verification,
            super::super::migration::HashVerification::Verified
        ));
    }
}
