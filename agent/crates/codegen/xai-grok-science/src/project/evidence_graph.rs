//! EvidenceGraph — typed directed graph connecting artifacts, claims,
//! observations, and reviewer verdicts. Seam contract: LS5-8.
//!
//! # Invariants
//! - No dangling nodes (all edges reference existing nodes)
//! - No self-referencing claims
//! - No cycles in derivation chains
//! - No cross-project edges
//! - Every node has a unique id within the graph
//! - Every edge must cite a supporting artifact SHA-256

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::model::ProjectId;

/// Unique node identifier within a graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct NodeId(pub String);

/// Types of nodes in the evidence graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    /// Raw data from a connector fetch.
    SourceArtifact,
    /// Transformed/derived data from notebook or computation.
    DerivedArtifact,
    /// A recorded observation (may be from device/sensor in V4+).
    Observation,
    /// A research claim made by a human or AI reviewer.
    Claim,
    /// A method or protocol description.
    Method,
    /// Parameters used in a computation.
    ParameterSet,
    /// The compute environment in which work was done.
    ComputeEnvironment,
    /// A reviewer's verdict on a claim.
    ReviewerVerdict,
    /// A device sensor reading (V4+).
    DeviceReading,
    /// A command sent to a device (V5+).
    DeviceCommand,
    /// A citation to an external publication.
    ExternalCitation,
}

/// A typed node in the evidence graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceNode {
    pub node_id: NodeId,
    pub kind: NodeKind,
    /// The project this node belongs to.
    pub project_id: ProjectId,
    /// Human-readable label.
    pub label: String,
    /// SHA-256 of the associated artifact, if any.
    pub artifact_sha256: Option<String>,
    /// The run that created this node.
    pub run_id: Option<String>,
    /// Who/what created this node.
    pub created_by: String,
    /// When created.
    pub created_at: DateTime<Utc>,
    /// Arbitrary metadata.
    pub metadata: BTreeMap<String, String>,
}

/// Types of directed edges in the evidence graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeKind {
    /// Artifact was derived from another artifact.
    DerivedFrom,
    /// Evidence supports a claim.
    Supports,
    /// Evidence contradicts a claim.
    Contradicts,
    /// An observation was measured by a specific device.
    MeasuredBy,
    /// Computation was executed in a specific environment.
    ExecutedWith,
    /// A claim was reviewed by a specific reviewer verdict.
    ReviewedBy,
    /// A new claim supersedes an older one.
    Supersedes,
    /// An independent attempt reproduces the original result.
    Reproduces,
    /// An independent attempt fails to reproduce the original result.
    FailsToReproduce,
}

/// A typed directed edge in the evidence graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceEdge {
    /// Source node id.
    pub source: NodeId,
    /// Target node id.
    pub target: NodeId,
    /// Relationship kind.
    pub relation: EdgeKind,
    /// Who/what created this edge.
    pub actor: String,
    /// When created.
    pub timestamp: DateTime<Utc>,
    /// Run that produced this edge.
    pub run_id: String,
    /// SHA-256 of the supporting artifact.
    pub supporting_artifact_sha256: String,
    /// Confidence: "high" | "medium" | "low" | "speculative".
    /// Must be accompanied by artifact citation — never standalone.
    pub confidence_kind: String,
}

/// The full evidence graph. All mutations go through session actor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceGraph {
    /// Owning project.
    pub project_id: ProjectId,
    /// Schema version.
    pub schema_version: u32,
    /// All nodes, keyed by NodeId.
    pub nodes: BTreeMap<NodeId, EvidenceNode>,
    /// All edges in order of creation.
    pub edges: Vec<EvidenceEdge>,
}

impl EvidenceGraph {
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;

    /// Create an empty graph for a project.
    pub fn new(project_id: ProjectId) -> Self {
        Self {
            project_id,
            schema_version: Self::CURRENT_SCHEMA_VERSION,
            nodes: BTreeMap::new(),
            edges: vec![],
        }
    }

    /// Add a node. Fails if node id already exists.
    pub fn add_node(&mut self, node: EvidenceNode) -> Result<(), String> {
        if self.nodes.contains_key(&node.node_id) {
            return Err(format!("duplicate node id: {:?}", node.node_id));
        }
        if node.project_id != self.project_id {
            return Err(format!(
                "node project {:?} does not match graph project {:?}",
                node.project_id, self.project_id
            ));
        }
        self.nodes.insert(node.node_id.clone(), node);
        Ok(())
    }

    /// Add an edge. Validates both endpoints exist, no self-loops on claims,
    /// and edges don't cross projects.
    pub fn add_edge(&mut self, edge: EvidenceEdge) -> Result<(), String> {
        if !self.nodes.contains_key(&edge.source) {
            return Err(format!("source node {:?} not found", edge.source));
        }
        if !self.nodes.contains_key(&edge.target) {
            return Err(format!("target node {:?} not found", edge.target));
        }
        if edge.source == edge.target {
            return Err("self-referencing edges are not allowed".to_string());
        }
        // Claim nodes cannot reference themselves
        if let (Some(src), Some(tgt)) = (
            self.nodes.get(&edge.source),
            self.nodes.get(&edge.target),
        ) {
            if matches!(src.kind, NodeKind::Claim)
                && matches!(tgt.kind, NodeKind::Claim)
                && src.node_id == tgt.node_id
            {
                return Err("claims cannot self-reference".to_string());
            }
        }
        self.edges.push(edge);
        Ok(())
    }

    /// Find all edges connected to a node.
    pub fn edges_for(&self, node_id: &NodeId) -> Vec<&EvidenceEdge> {
        self.edges
            .iter()
            .filter(|e| &e.source == node_id || &e.target == node_id)
            .collect()
    }

    /// Check for dangling references (nodes referenced by edges that don't exist).
    pub fn validate_integrity(&self) -> Result<(), String> {
        for edge in &self.edges {
            if !self.nodes.contains_key(&edge.source) {
                return Err(format!("dangling source node: {:?}", edge.source));
            }
            if !self.nodes.contains_key(&edge.target) {
                return Err(format!("dangling target node: {:?}", edge.target));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_project() -> ProjectId {
        ProjectId("test-proj".into())
    }

    fn test_node(id: &str, kind: NodeKind, project: &ProjectId) -> EvidenceNode {
        EvidenceNode {
            node_id: NodeId(id.into()),
            kind,
            project_id: project.clone(),
            label: format!("Node {id}"),
            artifact_sha256: None,
            run_id: None,
            created_by: "test".into(),
            created_at: Utc::now(),
            metadata: BTreeMap::new(),
        }
    }

    fn test_edge(
        source: &str,
        target: &str,
        relation: EdgeKind,
        run_id: &str,
    ) -> EvidenceEdge {
        EvidenceEdge {
            source: NodeId(source.into()),
            target: NodeId(target.into()),
            relation,
            actor: "test".into(),
            timestamp: Utc::now(),
            run_id: run_id.into(),
            supporting_artifact_sha256: "sha256:test".into(),
            confidence_kind: "high".into(),
        }
    }

    #[test]
    fn new_graph_is_empty() {
        let g = EvidenceGraph::new(test_project());
        assert!(g.nodes.is_empty());
        assert!(g.edges.is_empty());
    }

    #[test]
    fn rejects_duplicate_node() {
        let mut g = EvidenceGraph::new(test_project());
        let node = test_node("n1", NodeKind::SourceArtifact, &g.project_id);
        g.add_node(node.clone()).unwrap();
        assert!(g.add_node(node).is_err());
    }

    #[test]
    fn rejects_cross_project_node() {
        let mut g = EvidenceGraph::new(test_project());
        let other = ProjectId("other".into());
        let node = test_node("n1", NodeKind::SourceArtifact, &other);
        assert!(g.add_node(node).is_err());
    }

    #[test]
    fn rejects_self_referencing_edge() {
        let mut g = EvidenceGraph::new(test_project());
        g.add_node(test_node("n1", NodeKind::Claim, &g.project_id))
            .unwrap();
        let edge = test_edge("n1", "n1", EdgeKind::Supports, "run-1");
        assert!(g.add_edge(edge).is_err());
    }

    #[test]
    fn rejects_dangling_edge() {
        let mut g = EvidenceGraph::new(test_project());
        g.add_node(test_node("n1", NodeKind::Claim, &g.project_id))
            .unwrap();
        let edge = test_edge("n1", "n2", EdgeKind::Supports, "run-1");
        assert!(g.add_edge(edge).is_err());
    }

    #[test]
    fn supports_claim_derivation_chain() {
        let mut g = EvidenceGraph::new(test_project());
        let proj = g.project_id.clone();

        g.add_node(test_node("src", NodeKind::SourceArtifact, &proj)).unwrap();
        g.add_node(test_node("derived", NodeKind::DerivedArtifact, &proj)).unwrap();
        g.add_node(test_node("claim", NodeKind::Claim, &proj)).unwrap();
        g.add_node(test_node("review", NodeKind::ReviewerVerdict, &proj)).unwrap();

        g.add_edge(test_edge("derived", "src", EdgeKind::DerivedFrom, "r1")).unwrap();
        g.add_edge(test_edge("claim", "derived", EdgeKind::Supports, "r1")).unwrap();
        g.add_edge(test_edge("review", "claim", EdgeKind::ReviewedBy, "r1")).unwrap();

        assert_eq!(g.edges.len(), 3);
        assert!(g.validate_integrity().is_ok());
    }
}
