//! Durable store for ResearchProject + EvidenceGraph + Claims (WP-2 product path).
//!
//! Layout under `root/projects/{project_id}/`:
//!   project.json
//!   graph.json
//!   claims/{claim_id}.json
//!
//! Records only — SessionActor remains sole execution authority.

use super::claim::{Claim, ClaimStatus};
use super::evidence_graph::{
    EdgeKind, EvidenceEdge, EvidenceGraph, EvidenceNode, NodeId, NodeKind,
};
use super::model::{OwnerId, ProjectId, ProjectStatus, ResearchProject};
use crate::features::{FeatureGates, ScienceFeature};
use crate::{Result, ScienceError};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

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

    pub fn save_graph(&self, graph: &EvidenceGraph) -> Result<()> {
        self.gates.require(ScienceFeature::EvidenceGraph)?;
        let dir = self.ensure_dir(&graph.project_id)?;
        Self::write_json(&dir.join("graph.json"), graph)
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

    /// Attach a SourceArtifact node and Supports edge to a claim.
    /// Requires non-empty artifact_sha256 (fail closed).
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
        if sha.len() < 16 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ScienceError::Invalid(
                "artifact_sha256 must be hex (min 16 chars)".into(),
            ));
        }
        let mut claim = self.load_claim(project_id, claim_id)?;
        let claim_node = claim
            .evidence_node_id
            .clone()
            .ok_or_else(|| ScienceError::Invalid("claim missing evidence node".into()))?;

        let mut graph = self.load_graph(project_id)?;
        let art_node_id = NodeId(format!("art-{}", &sha[..16]));
        if !graph.nodes.contains_key(&art_node_id) {
            let art_node = EvidenceNode {
                node_id: art_node_id.clone(),
                kind: NodeKind::SourceArtifact,
                project_id: project_id.clone(),
                label: label.into(),
                artifact_sha256: Some(sha.clone()),
                run_id: run_id.clone(),
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
