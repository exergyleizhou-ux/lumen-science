//! Durable store for ResearchProject + EvidenceGraph + Claims (WP-2 product path).
//!
//! Layout under `root/projects/{project_id}/`:
//!   project.json
//!   graph.json
//!   artifacts.json
//!   claims/{claim_id}.json
//!
//! Records only — SessionActor remains sole execution authority.

use super::claim::{Claim, ClaimStatus};
use super::evidence_graph::{
    EdgeKind, EvidenceEdge, EvidenceGraph, EvidenceNode, NodeId, NodeKind, validate_sha256_hex,
};
use super::model::{OwnerId, ProjectId, ProjectStatus, ResearchProject};
use crate::features::{FeatureGates, ScienceFeature};
use crate::{Result, ScienceError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

/// An artifact digest that has been registered against a project.
///
/// `attach_evidence` refuses to cite a digest that is not registered here:
/// an evidence graph must never point at an artifact nobody produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredArtifact {
    pub project_id: ProjectId,
    /// Canonical digest: exactly 64 lowercase hex characters.
    pub sha256: String,
    pub label: String,
    pub run_id: Option<String>,
    pub registered_by: String,
    pub registered_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ProjectStore {
    root: PathBuf,
    gates: FeatureGates,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectBundle {
    pub project: ResearchProject,
    pub graph: EvidenceGraph,
    pub claims: Vec<Claim>,
}

impl ProjectStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            gates: FeatureGates::default(),
        }
    }

    pub fn with_gates(mut self, gates: FeatureGates) -> Self {
        self.gates = gates;
        self
    }

    pub fn gates(&self) -> &FeatureGates {
        &self.gates
    }

    pub fn gates_mut(&mut self) -> &mut FeatureGates {
        &mut self.gates
    }

    fn project_dir(&self, id: &ProjectId) -> PathBuf {
        self.root.join("projects").join(&id.0)
    }

    fn ensure_dir(&self, id: &ProjectId) -> Result<PathBuf> {
        let d = self.project_dir(id);
        fs::create_dir_all(d.join("claims"))?;
        Ok(d)
    }

    fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(value)?;
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, &bytes)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
        let bytes = fs::read(path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Create a draft research project with empty evidence graph.
    pub fn create_project(
        &self,
        owner_id: impl Into<String>,
        title: impl Into<String>,
        research_question: impl Into<String>,
    ) -> Result<ResearchProject> {
        self.gates.require(ScienceFeature::ResearchProject)?;
        let project_id = ProjectId(Uuid::now_v7().to_string());
        let mut project = ResearchProject::new(
            project_id.clone(),
            OwnerId(owner_id.into()),
            title.into(),
            research_question.into(),
        );
        let graph = EvidenceGraph::new(project_id.clone());
        project.evidence_graph_id = Some(format!("graph-{}", project_id.0));
        let dir = self.ensure_dir(&project_id)?;
        Self::write_json(&dir.join("project.json"), &project)?;
        Self::write_json(&dir.join("graph.json"), &graph)?;
        Ok(project)
    }

    pub fn load_project(&self, project_id: &ProjectId) -> Result<ResearchProject> {
        self.gates.require(ScienceFeature::ResearchProject)?;
        let path = self.project_dir(project_id).join("project.json");
        if !path.is_file() {
            return Err(ScienceError::Invalid(format!(
                "project not found: {}",
                project_id.0
            )));
        }
        Self::read_json(&path)
    }

    pub fn save_project(&self, project: &ResearchProject) -> Result<()> {
        self.gates.require(ScienceFeature::ResearchProject)?;
        let dir = self.ensure_dir(&project.project_id)?;
        Self::write_json(&dir.join("project.json"), project)
    }

    pub fn load_graph(&self, project_id: &ProjectId) -> Result<EvidenceGraph> {
        self.gates.require(ScienceFeature::EvidenceGraph)?;
        let path = self.project_dir(project_id).join("graph.json");
        if !path.is_file() {
            return Ok(EvidenceGraph::new(project_id.clone()));
        }
        Self::read_json(&path)
    }

    /// Persist a graph. Fails closed if the graph violates its structural
    /// invariants (dangling endpoint, self-edge, derivation cycle, or a
    /// non-canonical artifact digest), so a corrupt or legacy graph can never
    /// be written back under a new mutation.
    pub fn save_graph(&self, graph: &EvidenceGraph) -> Result<()> {
        self.gates.require(ScienceFeature::EvidenceGraph)?;
        graph
            .validate_integrity()
            .map_err(|error| ScienceError::Invalid(format!("evidence graph invalid: {error}")))?;
        let dir = self.ensure_dir(&graph.project_id)?;
        Self::write_json(&dir.join("graph.json"), graph)
    }

    // ── Artifact registry ─────────────────────────────────────────

    fn artifacts_path(&self, project_id: &ProjectId) -> PathBuf {
        self.project_dir(project_id).join("artifacts.json")
    }

    /// All artifact digests registered against a project, keyed by digest.
    pub fn list_artifacts(
        &self,
        project_id: &ProjectId,
    ) -> Result<BTreeMap<String, RegisteredArtifact>> {
        let path = self.artifacts_path(project_id);
        if !path.is_file() {
            return Ok(BTreeMap::new());
        }
        Self::read_json(&path)
    }

    /// Register an artifact digest against a project so evidence may cite it.
    ///
    /// Idempotent: re-registering the same digest for the same project returns
    /// the existing record rather than duplicating it.
    pub fn register_artifact(
        &self,
        project_id: &ProjectId,
        owner_id: &str,
        artifact_sha256: impl Into<String>,
        label: impl Into<String>,
        run_id: Option<String>,
    ) -> Result<RegisteredArtifact> {
        self.gates.require(ScienceFeature::EvidenceGraph)?;
        let project = self.load_project(project_id)?;
        if project.owner_id.0 != owner_id {
            return Err(ScienceError::Ownership);
        }
        let sha = artifact_sha256.into();
        validate_sha256_hex(&sha).map_err(ScienceError::Invalid)?;
        let mut registry = self.list_artifacts(project_id)?;
        if let Some(existing) = registry.get(&sha) {
            if existing.project_id != *project_id {
                return Err(ScienceError::Invalid(format!(
                    "registered artifact {sha} is bound to project {}, not {}",
                    existing.project_id.0, project_id.0
                )));
            }
            return Ok(existing.clone());
        }
        let record = RegisteredArtifact {
            project_id: project_id.clone(),
            sha256: sha.clone(),
            label: label.into(),
            run_id,
            registered_by: owner_id.to_string(),
            registered_at: Utc::now(),
        };
        registry.insert(sha, record.clone());
        self.ensure_dir(project_id)?;
        Self::write_json(&self.artifacts_path(project_id), &registry)?;
        Ok(record)
    }

    /// The first other project that has registered this digest, if any.
    /// Used only to turn "unknown artifact" into a precise cross-project error.
    fn locate_artifact_elsewhere(
        &self,
        sha: &str,
        exclude: &ProjectId,
    ) -> Result<Option<ProjectId>> {
        let root = self.root.join("projects");
        if !root.is_dir() {
            return Ok(None);
        }
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let path = entry.path().join("artifacts.json");
            if !path.is_file() {
                continue;
            }
            let registry: BTreeMap<String, RegisteredArtifact> = match Self::read_json(&path) {
                Ok(registry) => registry,
                // A damaged sibling registry must not mask the real error.
                Err(_) => continue,
            };
            if let Some(record) = registry.get(sha)
                && record.project_id != *exclude
            {
                return Ok(Some(record.project_id.clone()));
            }
        }
        Ok(None)
    }

    /// Resolve a digest to its registration in this project, failing closed
    /// with a precise reason when it is unknown or owned by another project.
    fn require_registered_artifact(
        &self,
        project_id: &ProjectId,
        sha: &str,
    ) -> Result<RegisteredArtifact> {
        let registry = self.list_artifacts(project_id)?;
        match registry.get(sha) {
            Some(record) if record.project_id == *project_id => Ok(record.clone()),
            Some(record) => Err(ScienceError::Invalid(format!(
                "artifact {sha} is registered to project {}, not {}",
                record.project_id.0, project_id.0
            ))),
            None => match self.locate_artifact_elsewhere(sha, project_id)? {
                Some(other) => Err(ScienceError::Invalid(format!(
                    "artifact {sha} is registered to project {}, not {} — evidence may not cross projects",
                    other.0, project_id.0
                ))),
                None => Err(ScienceError::Invalid(format!(
                    "artifact {sha} is not registered in project {}; register it before citing it as evidence",
                    project_id.0
                ))),
            },
        }
    }

    pub fn transition_project(
        &self,
        project_id: &ProjectId,
        owner_id: &str,
        new_status: ProjectStatus,
    ) -> Result<ResearchProject> {
        let mut project = self.load_project(project_id)?;
        if project.owner_id.0 != owner_id {
            return Err(ScienceError::Ownership);
        }
        project
            .transition_to(new_status)
            .map_err(ScienceError::Invalid)?;
        self.save_project(&project)?;
        Ok(project)
    }

    /// Propose a claim node in the evidence graph.
    pub fn propose_claim(
        &self,
        project_id: &ProjectId,
        owner_id: &str,
        statement: impl Into<String>,
        proposed_by: impl Into<String>,
    ) -> Result<Claim> {
        self.gates.require(ScienceFeature::ClaimLifecycle)?;
        let project = self.load_project(project_id)?;
        if project.owner_id.0 != owner_id {
            return Err(ScienceError::Ownership);
        }
        let claim_id = format!("claim-{}", Uuid::now_v7());
        let node_id = NodeId(format!("node-{}", claim_id));
        let mut claim = Claim::new(
            claim_id.clone(),
            project_id.clone(),
            statement.into(),
            proposed_by.into(),
        );
        claim.evidence_node_id = Some(node_id.clone());

        let mut graph = self.load_graph(project_id)?;
        let node = EvidenceNode {
            node_id: node_id.clone(),
            kind: NodeKind::Claim,
            project_id: project_id.clone(),
            label: claim.statement.clone(),
            artifact_sha256: None,
            run_id: None,
            created_by: claim.proposed_by.clone(),
            created_at: Utc::now(),
            metadata: BTreeMap::new(),
        };
        graph
            .add_node(node)
            .map_err(ScienceError::Invalid)?;
        self.save_graph(&graph)?;

        let dir = self.ensure_dir(project_id)?;
        Self::write_json(&dir.join("claims").join(format!("{claim_id}.json")), &claim)?;
        Ok(claim)
    }

    pub fn load_claim(&self, project_id: &ProjectId, claim_id: &str) -> Result<Claim> {
        self.gates.require(ScienceFeature::ClaimLifecycle)?;
        let path = self
            .project_dir(project_id)
            .join("claims")
            .join(format!("{claim_id}.json"));
        if !path.is_file() {
            return Err(ScienceError::Invalid(format!(
                "claim not found: {claim_id}"
            )));
        }
        Self::read_json(&path)
    }

    pub fn list_claims(&self, project_id: &ProjectId) -> Result<Vec<Claim>> {
        self.gates.require(ScienceFeature::ClaimLifecycle)?;
        let dir = self.project_dir(project_id).join("claims");
        if !dir.is_dir() {
            return Ok(vec![]);
        }
        let mut out: Vec<Claim> = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let claim: Claim = Self::read_json(&path)?;
                out.push(claim);
            }
        }
        out.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));
        Ok(out)
    }

    /// The evidence-graph node id for an artifact digest.
    ///
    /// The identity uses the **full** digest. A truncated identity would make
    /// two distinct artifacts that happen to share a prefix collapse onto one
    /// evidence node, silently re-pointing every citation of one at the other.
    pub fn artifact_node_id(sha256: &str) -> NodeId {
        NodeId(format!("art-{sha256}"))
    }

    /// Attach a SourceArtifact node and Supports edge to a claim.
    ///
    /// Fail closed on: a non-canonical digest (must be exactly 64 lowercase
    /// hex), a digest that is not registered in this project, a digest
    /// registered to a different project, and any graph mutation that would
    /// leave a dangling endpoint, self-edge, or derivation cycle.
    pub fn attach_evidence(
        &self,
        project_id: &ProjectId,
        owner_id: &str,
        claim_id: &str,
        artifact_sha256: impl Into<String>,
        label: impl Into<String>,
        run_id: Option<String>,
    ) -> Result<(Claim, EvidenceGraph)> {
        self.gates.require(ScienceFeature::EvidenceGraph)?;
        self.gates.require(ScienceFeature::ClaimLifecycle)?;
        let project = self.load_project(project_id)?;
        if project.owner_id.0 != owner_id {
            return Err(ScienceError::Ownership);
        }
        let sha = artifact_sha256.into();
        validate_sha256_hex(&sha).map_err(ScienceError::Invalid)?;
        let registered = self.require_registered_artifact(project_id, &sha)?;
        let mut claim = self.load_claim(project_id, claim_id)?;
        if claim.project_id != *project_id {
            return Err(ScienceError::Invalid(format!(
                "claim {claim_id} belongs to project {}, not {}",
                claim.project_id.0, project_id.0
            )));
        }
        let claim_node = claim
            .evidence_node_id
            .clone()
            .ok_or_else(|| ScienceError::Invalid("claim missing evidence node".into()))?;

        let mut graph = self.load_graph(project_id)?;
        if !graph.nodes.contains_key(&claim_node) {
            return Err(ScienceError::Invalid(format!(
                "claim node {} is not in the evidence graph; run recover_project first",
                claim_node.0
            )));
        }
        let art_node_id = Self::artifact_node_id(&sha);
        // A graph written before digests were canonical identifies the same
        // artifact under a truncated node id. Refuse rather than silently
        // creating a second node for one artifact.
        if let Some(stale) = graph.nodes.values().find(|node| {
            node.artifact_sha256.as_deref() == Some(sha.as_str()) && node.node_id != art_node_id
        }) {
            return Err(ScienceError::Invalid(format!(
                "artifact {sha} already appears as legacy node {}; migrate the graph to full-digest node ids",
                stale.node_id.0
            )));
        }
        if !graph.nodes.contains_key(&art_node_id) {
            let art_node = EvidenceNode {
                node_id: art_node_id.clone(),
                kind: NodeKind::SourceArtifact,
                project_id: project_id.clone(),
                label: label.into(),
                artifact_sha256: Some(sha.clone()),
                run_id: run_id.clone().or_else(|| registered.run_id.clone()),
                created_by: owner_id.to_string(),
                created_at: Utc::now(),
                metadata: BTreeMap::new(),
            };
            graph
                .add_node(art_node)
                .map_err(ScienceError::Invalid)?;
        }

        let edge = EvidenceEdge {
            source: art_node_id,
            target: claim_node,
            relation: EdgeKind::Supports,
            actor: owner_id.to_string(),
            timestamp: Utc::now(),
            run_id: run_id.unwrap_or_else(|| "none".into()),
            supporting_artifact_sha256: sha,
            confidence_kind: "high".into(),
        };
        graph.add_edge(edge).map_err(ScienceError::Invalid)?;
        self.save_graph(&graph)?;

        if claim.status == ClaimStatus::Proposed {
            claim
                .transition_to(ClaimStatus::EvidenceAttached)
                .map_err(ScienceError::Invalid)?;
        }
        let dir = self.ensure_dir(project_id)?;
        Self::write_json(
            &dir.join("claims").join(format!("{}.json", claim.claim_id)),
            &claim,
        )?;
        Ok((claim, graph))
    }

    pub fn load_bundle(&self, project_id: &ProjectId) -> Result<ProjectBundle> {
        Ok(ProjectBundle {
            project: self.load_project(project_id)?,
            graph: self.load_graph(project_id)?,
            claims: self.list_claims(project_id)?,
        })
    }

    pub fn list_projects(&self) -> Result<Vec<ResearchProject>> {
        self.gates.require(ScienceFeature::ResearchProject)?;
        let root = self.root.join("projects");
        if !root.is_dir() {
            return Ok(vec![]);
        }
        let mut out: Vec<ResearchProject> = Vec::new();
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let pj = entry.path().join("project.json");
            if pj.is_file() {
                let project: ResearchProject = Self::read_json(&pj)?;
                out.push(project);
            }
        }
        out.sort_by(|a, b| a.project_id.0.cmp(&b.project_id.0));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn create_claim_attach_evidence_roundtrip() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store
            .create_project("owner-1", "Demo", "Does EcoRI cut?")
            .unwrap();
        assert_eq!(p.status, ProjectStatus::Draft);

        let claim = store
            .propose_claim(&p.project_id, "owner-1", "EcoRI site present", "scientist")
            .unwrap();
        assert_eq!(claim.status, ClaimStatus::Proposed);

        let sha = "a".repeat(64);
        // Evidence may only cite a registered artifact, so register it first.
        store
            .register_artifact(
                &p.project_id,
                "owner-1",
                sha.clone(),
                "seq analysis",
                Some("run-1".into()),
            )
            .unwrap();
        let (claim2, graph) = store
            .attach_evidence(
                &p.project_id,
                "owner-1",
                &claim.claim_id,
                sha,
                "seq analysis",
                Some("run-1".into()),
            )
            .unwrap();
        assert_eq!(claim2.status, ClaimStatus::EvidenceAttached);
        assert!(graph.nodes.len() >= 2);
        assert!(!graph.edges.is_empty());

        // ownership fail closed
        assert!(matches!(
            store.propose_claim(&p.project_id, "other", "x", "x"),
            Err(ScienceError::Ownership)
        ));

        // bad hash fail closed
        assert!(store
            .attach_evidence(&p.project_id, "owner-1", &claim.claim_id, "nope", "x", None)
            .is_err());
    }

    /// Set up a project with one claim and one registered artifact.
    fn seeded(dir: &std::path::Path) -> (ProjectStore, ResearchProject, Claim, String) {
        let store = ProjectStore::new(dir);
        let project = store.create_project("o", "T", "Q").unwrap();
        let claim = store
            .propose_claim(&project.project_id, "o", "statement", "sci")
            .unwrap();
        let sha = "a".repeat(64);
        store
            .register_artifact(&project.project_id, "o", sha.clone(), "art", None)
            .unwrap();
        (store, project, claim, sha)
    }

    // ── Defect A regression: digest identity + registry ────────────

    /// The defect: node identity was `art-{&sha[..16]}`, so two distinct
    /// artifacts sharing a 16-character prefix collapsed onto one node.
    #[test]
    fn distinct_digests_sharing_a_prefix_get_distinct_nodes() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.create_project("o", "T", "Q").unwrap();
        let claim = store.propose_claim(&p.project_id, "o", "s", "sci").unwrap();

        let prefix = "0123456789abcdef";
        let first = format!("{prefix}{}", "1".repeat(48));
        let second = format!("{prefix}{}", "2".repeat(48));
        for sha in [&first, &second] {
            store
                .register_artifact(&p.project_id, "o", sha.clone(), "art", None)
                .unwrap();
            store
                .attach_evidence(&p.project_id, "o", &claim.claim_id, sha.clone(), "l", None)
                .unwrap();
        }

        let graph = store.load_graph(&p.project_id).unwrap();
        assert!(graph.nodes.contains_key(&ProjectStore::artifact_node_id(&first)));
        assert!(graph.nodes.contains_key(&ProjectStore::artifact_node_id(&second)));
        // claim node + two distinct artifact nodes
        assert_eq!(graph.nodes.len(), 3, "prefix collision collapsed two artifacts");
        assert_eq!(graph.edges.len(), 2);
    }

    #[test]
    fn attach_evidence_rejects_non_canonical_digests() {
        let dir = tempdir().unwrap();
        let (store, p, claim, _) = seeded(dir.path());
        for bad in [
            "a".repeat(16),  // the old minimum — now rejected
            "a".repeat(63),  // too short
            "a".repeat(65),  // too long
            "A".repeat(64),  // uppercase is not normalised
            "g".repeat(64),  // non-hex
            format!("sha256:{}", "a".repeat(57)),
            String::new(),
        ] {
            let error = store
                .attach_evidence(&p.project_id, "o", &claim.claim_id, bad.clone(), "l", None)
                .unwrap_err();
            assert!(
                matches!(&error, ScienceError::Invalid(m) if m.contains("artifact digest")),
                "digest {bad:?} was not rejected as malformed: {error}"
            );
        }
    }

    #[test]
    fn attach_evidence_rejects_unregistered_artifact() {
        let dir = tempdir().unwrap();
        let (store, p, claim, _) = seeded(dir.path());
        let unknown = "b".repeat(64);
        let error = store
            .attach_evidence(&p.project_id, "o", &claim.claim_id, unknown, "l", None)
            .unwrap_err();
        assert!(
            matches!(&error, ScienceError::Invalid(m) if m.contains("not registered")),
            "unexpected: {error}"
        );
    }

    #[test]
    fn attach_evidence_rejects_cross_project_artifact() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let a = store.create_project("o", "A", "Q").unwrap();
        let b = store.create_project("o", "B", "Q").unwrap();
        let claim = store.propose_claim(&b.project_id, "o", "s", "sci").unwrap();
        let sha = "c".repeat(64);
        store
            .register_artifact(&a.project_id, "o", sha.clone(), "art", None)
            .unwrap();

        let error = store
            .attach_evidence(&b.project_id, "o", &claim.claim_id, sha, "l", None)
            .unwrap_err();
        assert!(
            matches!(&error, ScienceError::Invalid(m) if m.contains("cross projects")),
            "unexpected: {error}"
        );
    }

    #[test]
    fn attach_evidence_rejects_unknown_claim_and_foreign_owner() {
        let dir = tempdir().unwrap();
        let (store, p, _claim, sha) = seeded(dir.path());
        assert!(matches!(
            store.attach_evidence(&p.project_id, "intruder", "claim-x", sha.clone(), "l", None),
            Err(ScienceError::Ownership)
        ));
        assert!(store
            .attach_evidence(&p.project_id, "o", "claim-does-not-exist", sha, "l", None)
            .is_err());
    }

    #[test]
    fn register_artifact_validates_digest_owner_and_is_idempotent() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.create_project("o", "T", "Q").unwrap();

        assert!(store
            .register_artifact(&p.project_id, "o", "a".repeat(16), "l", None)
            .is_err());
        assert!(store
            .register_artifact(&p.project_id, "o", "A".repeat(64), "l", None)
            .is_err());
        assert!(matches!(
            store.register_artifact(&p.project_id, "intruder", "a".repeat(64), "l", None),
            Err(ScienceError::Ownership)
        ));

        let sha = "a".repeat(64);
        let first = store
            .register_artifact(&p.project_id, "o", sha.clone(), "l", None)
            .unwrap();
        let again = store
            .register_artifact(&p.project_id, "o", sha.clone(), "other label", None)
            .unwrap();
        assert_eq!(first, again, "re-registration must be idempotent");
        assert_eq!(store.list_artifacts(&p.project_id).unwrap().len(), 1);
    }

    /// A store written before digests were canonical must fail loudly on the
    /// next mutation rather than being silently re-persisted.
    #[test]
    fn legacy_graph_with_truncated_digest_is_rejected_on_save() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.create_project("o", "T", "Q").unwrap();
        let mut graph = store.load_graph(&p.project_id).unwrap();
        graph.nodes.insert(
            NodeId("art-aaaaaaaaaaaaaaaa".into()),
            EvidenceNode {
                node_id: NodeId("art-aaaaaaaaaaaaaaaa".into()),
                kind: NodeKind::SourceArtifact,
                project_id: p.project_id.clone(),
                label: "legacy".into(),
                artifact_sha256: Some("a".repeat(16)),
                run_id: None,
                created_by: "o".into(),
                created_at: Utc::now(),
                metadata: BTreeMap::new(),
            },
        );
        let error = store.save_graph(&graph).unwrap_err();
        assert!(
            matches!(&error, ScienceError::Invalid(m) if m.contains("artifact digest")),
            "unexpected: {error}"
        );
    }

    /// A legacy graph that carries a full digest under a truncated node id is
    /// refused too, rather than growing a second node for one artifact.
    #[test]
    fn legacy_truncated_node_id_blocks_attach() {
        let dir = tempdir().unwrap();
        let (store, p, claim, sha) = seeded(dir.path());
        let mut graph = store.load_graph(&p.project_id).unwrap();
        let legacy = NodeId(format!("art-{}", &sha[..16]));
        graph.nodes.insert(
            legacy.clone(),
            EvidenceNode {
                node_id: legacy,
                kind: NodeKind::SourceArtifact,
                project_id: p.project_id.clone(),
                label: "legacy".into(),
                artifact_sha256: Some(sha.clone()),
                run_id: None,
                created_by: "o".into(),
                created_at: Utc::now(),
                metadata: BTreeMap::new(),
            },
        );
        store.save_graph(&graph).unwrap();

        let error = store
            .attach_evidence(&p.project_id, "o", &claim.claim_id, sha, "l", None)
            .unwrap_err();
        assert!(
            matches!(&error, ScienceError::Invalid(m) if m.contains("legacy node")),
            "unexpected: {error}"
        );
    }

    #[test]
    fn feature_gate_blocks_when_disabled() {
        let dir = tempdir().unwrap();
        let mut store = ProjectStore::new(dir.path());
        store
            .gates_mut()
            .set(ScienceFeature::ResearchProject, crate::features::GateState::Disabled);
        assert!(matches!(
            store.create_project("o", "t", "q"),
            Err(ScienceError::FeatureDisabled(_))
        ));
    }

    #[test]
    fn project_transition_lifecycle() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.create_project("o", "t", "q").unwrap();
        let p = store
            .transition_project(&p.project_id, "o", ProjectStatus::Planned)
            .unwrap();
        assert_eq!(p.status, ProjectStatus::Planned);
        let p = store
            .transition_project(&p.project_id, "o", ProjectStatus::Active)
            .unwrap();
        assert_eq!(p.status, ProjectStatus::Active);
    }
}
