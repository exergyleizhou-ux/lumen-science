//! EvidenceGraph — typed directed graph connecting artifacts, claims,
//! observations, and reviewer verdicts. Seam contract: LS5-8.
//!
//! # Invariants
//! - No dangling nodes (all edges reference existing nodes)
//! - No self-referencing claims
//! - No cycles in derivation chains
//! - No cross-project edges
//! - Every node has a unique id within the graph
//! - Every edge must cite a supporting artifact SHA-256 in canonical form
//!   (exactly 64 lowercase hex characters)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::model::ProjectId;

/// Canonical length of a hex-encoded SHA-256 digest.
pub const SHA256_HEX_LEN: usize = 64;

/// Validate an artifact digest in canonical form: exactly 64 lowercase hex
/// characters.
///
/// Fail closed on every other shape. In particular:
/// - short digests are rejected, because a truncated digest is not a unique
///   identity: two distinct artifacts sharing a prefix would collapse onto one
///   evidence node;
/// - uppercase is rejected rather than normalized, so a digest is always
///   byte-comparable and node identity derived from it cannot fork into two
///   spellings of the same artifact.
pub fn validate_sha256_hex(value: &str) -> Result<(), String> {
    if value.len() != SHA256_HEX_LEN {
        return Err(format!(
            "artifact digest must be exactly {SHA256_HEX_LEN} hex characters, got {}",
            value.len()
        ));
    }
    if !value
        .bytes()
        .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(format!(
            "artifact digest must be lowercase hex [0-9a-f], got {value:?}"
        ));
    }
    Ok(())
}

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

impl EdgeKind {
    /// Whether this edge participates in the provenance/derivation chain.
    ///
    /// A cycle over these edges means an artifact or claim ultimately derives
    /// its support from itself — circular reasoning, which the graph rejects.
    ///
    /// `Contradicts`, `ReviewedBy`, `Reproduces` and `FailsToReproduce` are
    /// deliberately excluded: they are assertions *about* a node rather than
    /// derivations *from* it, and mutual assertions (A contradicts B while B
    /// contradicts A) are legitimate.
    pub fn is_derivation(&self) -> bool {
        matches!(
            self,
            Self::DerivedFrom
                | Self::Supports
                | Self::Supersedes
                | Self::ExecutedWith
                | Self::MeasuredBy
        )
    }
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
        if let Some(sha) = &node.artifact_sha256 {
            validate_sha256_hex(sha)
                .map_err(|error| format!("node {:?}: {error}", node.node_id))?;
        }
        self.nodes.insert(node.node_id.clone(), node);
        Ok(())
    }

    /// Add an edge. Validates the cited digest, that both endpoints exist, no
    /// self-loops, no derivation cycles, and that edges don't cross projects.
    pub fn add_edge(&mut self, edge: EvidenceEdge) -> Result<(), String> {
        validate_sha256_hex(&edge.supporting_artifact_sha256).map_err(|error| {
            format!(
                "edge {:?} -> {:?}: {error}",
                edge.source, edge.target
            )
        })?;
        if !self.nodes.contains_key(&edge.source) {
            return Err(format!("source node {:?} not found", edge.source));
        }
        if !self.nodes.contains_key(&edge.target) {
            return Err(format!("target node {:?} not found", edge.target));
        }
        if edge.source == edge.target {
            return Err("self-referencing edges are not allowed".to_string());
        }
        if edge.relation.is_derivation()
            && self.derivation_reaches(&edge.target, &edge.source)
        {
            return Err(format!(
                "edge {:?} -> {:?} would close a derivation cycle",
                edge.source, edge.target
            ));
        }
        // Claim nodes cannot reference themselves
        if let (Some(src), Some(tgt)) = (
            self.nodes.get(&edge.source),
            self.nodes.get(&edge.target),
        )
            && matches!(src.kind, NodeKind::Claim)
                && matches!(tgt.kind, NodeKind::Claim)
                && src.node_id == tgt.node_id
            {
                return Err("claims cannot self-reference".to_string());
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

    /// Whether `target` is reachable from `start` by following derivation
    /// edges. Used to reject an edge that would close a derivation cycle.
    pub fn derivation_reaches(&self, start: &NodeId, target: &NodeId) -> bool {
        let mut seen: BTreeSet<&NodeId> = BTreeSet::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            if node == target {
                return true;
            }
            if !seen.insert(node) {
                continue;
            }
            for edge in &self.edges {
                if &edge.source == node && edge.relation.is_derivation() {
                    stack.push(&edge.target);
                }
            }
        }
        false
    }

    /// Find one derivation cycle already present in the graph, if any.
    /// Returns the node the cycle closes on.
    pub fn find_derivation_cycle(&self) -> Option<NodeId> {
        for edge in &self.edges {
            if !edge.relation.is_derivation() {
                continue;
            }
            if edge.source == edge.target {
                return Some(edge.source.clone());
            }
            // Reachability from target back to source means source -> target
            // closes a loop.
            if self.derivation_reaches_excluding(&edge.target, &edge.source, edge) {
                return Some(edge.source.clone());
            }
        }
        None
    }

    fn derivation_reaches_excluding(
        &self,
        start: &NodeId,
        target: &NodeId,
        skip: &EvidenceEdge,
    ) -> bool {
        let skip_key = (&skip.source, &skip.target, &skip.relation);
        let mut seen: BTreeSet<&NodeId> = BTreeSet::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            if node == target {
                return true;
            }
            if !seen.insert(node) {
                continue;
            }
            for edge in &self.edges {
                if (&edge.source, &edge.target, &edge.relation) == skip_key {
                    continue;
                }
                if &edge.source == node && edge.relation.is_derivation() {
                    stack.push(&edge.target);
                }
            }
        }
        false
    }

    /// Check the structural invariants a persisted graph must always satisfy:
    /// no dangling edge endpoints, no self-edges, no derivation cycles, and a
    /// canonical artifact digest on every node and edge that cites one.
    ///
    /// Graphs written before digests were canonicalized fail here with an
    /// explicit error rather than being silently re-persisted.
    pub fn validate_integrity(&self) -> Result<(), String> {
        for (node_id, node) in &self.nodes {
            if node.project_id != self.project_id {
                return Err(format!(
                    "node {node_id:?} belongs to project {:?}, graph is {:?}",
                    node.project_id, self.project_id
                ));
            }
            if let Some(sha) = &node.artifact_sha256 {
                validate_sha256_hex(sha).map_err(|error| format!("node {node_id:?}: {error}"))?;
            }
        }
        for edge in &self.edges {
            if !self.nodes.contains_key(&edge.source) {
                return Err(format!("dangling source node: {:?}", edge.source));
            }
            if !self.nodes.contains_key(&edge.target) {
                return Err(format!("dangling target node: {:?}", edge.target));
            }
            if edge.source == edge.target {
                return Err(format!("self-referencing edge on {:?}", edge.source));
            }
            validate_sha256_hex(&edge.supporting_artifact_sha256).map_err(|error| {
                format!("edge {:?} -> {:?}: {error}", edge.source, edge.target)
            })?;
        }
        if let Some(node) = self.find_derivation_cycle() {
            return Err(format!("derivation cycle through node {node:?}"));
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

    /// A canonical 64-lowercase-hex digest for fixtures.
    fn digest(seed: char) -> String {
        std::iter::repeat_n(seed, SHA256_HEX_LEN).collect()
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
            supporting_artifact_sha256: digest('a'),
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

    // ── Digest canonicalisation (defect A) ─────────────────────────

    #[test]
    fn validate_sha256_hex_accepts_only_canonical_form() {
        assert!(validate_sha256_hex(&digest('a')).is_ok());
        assert!(validate_sha256_hex(&"0123456789abcdef".repeat(4)).is_ok());
        // too short — a truncated digest is not an identity
        assert!(validate_sha256_hex(&"a".repeat(16)).is_err());
        assert!(validate_sha256_hex(&"a".repeat(63)).is_err());
        // too long
        assert!(validate_sha256_hex(&"a".repeat(65)).is_err());
        // uppercase is rejected, not normalised
        assert!(validate_sha256_hex(&"A".repeat(64)).is_err());
        // non-hex
        assert!(validate_sha256_hex(&"g".repeat(64)).is_err());
        assert!(validate_sha256_hex(&format!("sha256:{}", "a".repeat(57))).is_err());
        assert!(validate_sha256_hex("").is_err());
    }

    #[test]
    fn rejects_node_with_non_canonical_digest() {
        let mut g = EvidenceGraph::new(test_project());
        for bad in ["a".repeat(16), "A".repeat(64), "z".repeat(64), String::new()] {
            let mut node = test_node("n1", NodeKind::SourceArtifact, &g.project_id);
            node.artifact_sha256 = Some(bad.clone());
            let error = g.add_node(node).unwrap_err();
            assert!(error.contains("artifact digest"), "unexpected: {error}");
        }
        assert!(g.nodes.is_empty());
    }

    #[test]
    fn rejects_edge_with_non_canonical_digest() {
        let mut g = EvidenceGraph::new(test_project());
        g.add_node(test_node("n1", NodeKind::SourceArtifact, &g.project_id))
            .unwrap();
        g.add_node(test_node("n2", NodeKind::Claim, &g.project_id))
            .unwrap();
        for bad in ["a".repeat(16), "A".repeat(64), "sha256:x".to_string()] {
            let mut edge = test_edge("n1", "n2", EdgeKind::Supports, "run-1");
            edge.supporting_artifact_sha256 = bad;
            let error = g.add_edge(edge).unwrap_err();
            assert!(error.contains("artifact digest"), "unexpected: {error}");
        }
        assert!(g.edges.is_empty());
    }

    // ── Derivation cycles ──────────────────────────────────────────

    #[test]
    fn rejects_direct_derivation_cycle() {
        let mut g = EvidenceGraph::new(test_project());
        g.add_node(test_node("a", NodeKind::DerivedArtifact, &g.project_id))
            .unwrap();
        g.add_node(test_node("b", NodeKind::DerivedArtifact, &g.project_id))
            .unwrap();
        g.add_edge(test_edge("a", "b", EdgeKind::DerivedFrom, "r1"))
            .unwrap();
        let error = g
            .add_edge(test_edge("b", "a", EdgeKind::DerivedFrom, "r1"))
            .unwrap_err();
        assert!(error.contains("derivation cycle"), "unexpected: {error}");
        assert_eq!(g.edges.len(), 1);
    }

    #[test]
    fn rejects_transitive_derivation_cycle() {
        let mut g = EvidenceGraph::new(test_project());
        for id in ["a", "b", "c"] {
            g.add_node(test_node(id, NodeKind::DerivedArtifact, &g.project_id))
                .unwrap();
        }
        g.add_edge(test_edge("a", "b", EdgeKind::DerivedFrom, "r1"))
            .unwrap();
        g.add_edge(test_edge("b", "c", EdgeKind::Supports, "r1"))
            .unwrap();
        let error = g
            .add_edge(test_edge("c", "a", EdgeKind::Supersedes, "r1"))
            .unwrap_err();
        assert!(error.contains("derivation cycle"), "unexpected: {error}");
        assert!(g.validate_integrity().is_ok());
    }

    #[test]
    fn allows_mutual_contradiction_which_is_not_a_derivation() {
        let mut g = EvidenceGraph::new(test_project());
        g.add_node(test_node("a", NodeKind::Claim, &g.project_id))
            .unwrap();
        g.add_node(test_node("b", NodeKind::Claim, &g.project_id))
            .unwrap();
        g.add_edge(test_edge("a", "b", EdgeKind::Contradicts, "r1"))
            .unwrap();
        g.add_edge(test_edge("b", "a", EdgeKind::Contradicts, "r1"))
            .unwrap();
        assert!(g.validate_integrity().is_ok());
    }

    #[test]
    fn validate_integrity_detects_cycle_smuggled_past_add_edge() {
        let mut g = EvidenceGraph::new(test_project());
        g.add_node(test_node("a", NodeKind::DerivedArtifact, &g.project_id))
            .unwrap();
        g.add_node(test_node("b", NodeKind::DerivedArtifact, &g.project_id))
            .unwrap();
        // Bypass add_edge the way a deserialized legacy graph would.
        g.edges.push(test_edge("a", "b", EdgeKind::DerivedFrom, "r1"));
        g.edges.push(test_edge("b", "a", EdgeKind::DerivedFrom, "r1"));
        let error = g.validate_integrity().unwrap_err();
        assert!(error.contains("derivation cycle"), "unexpected: {error}");
    }

    #[test]
    fn validate_integrity_rejects_legacy_truncated_digest() {
        let mut g = EvidenceGraph::new(test_project());
        let mut node = test_node("art-aaaaaaaaaaaaaaaa", NodeKind::SourceArtifact, &g.project_id);
        node.artifact_sha256 = Some("a".repeat(16));
        g.nodes.insert(node.node_id.clone(), node);
        let error = g.validate_integrity().unwrap_err();
        assert!(error.contains("artifact digest"), "unexpected: {error}");
    }

    #[test]
    fn validate_integrity_rejects_self_edge_and_dangling_edge() {
        let mut g = EvidenceGraph::new(test_project());
        g.add_node(test_node("a", NodeKind::Claim, &g.project_id))
            .unwrap();
        g.edges.push(test_edge("a", "a", EdgeKind::Supports, "r1"));
        assert!(g.validate_integrity().unwrap_err().contains("self-referencing"));

        let mut g = EvidenceGraph::new(test_project());
        g.add_node(test_node("a", NodeKind::Claim, &g.project_id))
            .unwrap();
        g.edges.push(test_edge("ghost", "a", EdgeKind::Supports, "r1"));
        assert!(g.validate_integrity().unwrap_err().contains("dangling source"));
    }
}
