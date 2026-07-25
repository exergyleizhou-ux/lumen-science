//! Evidence query API and graph consistency checks.
//! Seam contracts: LS5-11, LS5-12.

use std::collections::BTreeSet;
use serde::{Deserialize, Serialize};

use super::evidence_graph::{EdgeKind, EvidenceEdge, EvidenceGraph, EvidenceNode, NodeId, NodeKind};
use super::model::ProjectId;

// ── Query types ────────────────────────────────────────────────────

/// A trace from a claim back to its source artifacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceTrace {
    pub claim_node_id: NodeId,
    pub trace_steps: Vec<TraceStep>,
    pub depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceStep {
    pub from: NodeId,
    pub to: NodeId,
    pub relation: EdgeKind,
    pub artifact_sha256: String,
}

/// Comparison result between two claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimComparison {
    pub claim_a: NodeId,
    pub claim_b: NodeId,
    pub shared_evidence: Vec<NodeId>,
    pub conflicting_evidence: Vec<NodeId>,
    pub supports_same_conclusion: bool,
}

/// Reproduction status for a claim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReproductionStatus {
    pub claim_node_id: NodeId,
    pub reproduction_attempts: Vec<ReproductionAttempt>,
    pub latest_status: ReproductionOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReproductionAttempt {
    pub run_id: String,
    pub timestamp: String,
    pub outcome: ReproductionOutcome,
    pub evidence_node_id: NodeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReproductionOutcome {
    Pending,
    Reproduced,
    FailedToReproduce,
    Inconclusive,
}

// ── Query API ──────────────────────────────────────────────────────

impl EvidenceGraph {
    /// Trace evidence from a claim back to source artifacts.
    pub fn trace_evidence(&self, claim_id: &NodeId) -> Result<EvidenceTrace, String> {
        let claim = self.nodes.get(claim_id)
            .ok_or_else(|| format!("claim node {:?} not found", claim_id))?;

        if !matches!(claim.kind, NodeKind::Claim) {
            return Err("trace_evidence requires a Claim node".to_string());
        }

        let mut steps = Vec::new();
        let mut visited = BTreeSet::new();
        self.trace_backward(claim_id, &mut steps, &mut visited, 0);

        Ok(EvidenceTrace {
            claim_node_id: claim_id.clone(),
            trace_steps: steps,
            depth: visited.len(),
        })
    }

    fn trace_backward(
        &self,
        node_id: &NodeId,
        steps: &mut Vec<TraceStep>,
        visited: &mut BTreeSet<NodeId>,
        depth: usize,
    ) {
        if depth > 100 || !visited.insert(node_id.clone()) {
            return; // cycle or too deep
        }

        for edge in &self.edges {
            if &edge.target == node_id {
                steps.push(TraceStep {
                    from: edge.source.clone(),
                    to: edge.target.clone(),
                    relation: edge.relation.clone(),
                    artifact_sha256: edge.supporting_artifact_sha256.clone(),
                });
                self.trace_backward(&edge.source, steps, visited, depth + 1);
            }
        }
    }

    /// Compare two claims — find shared and conflicting evidence.
    pub fn compare_claims(&self, a: &NodeId, b: &NodeId) -> Result<ClaimComparison, String> {
        let trace_a = self.trace_evidence(a)?;
        let trace_b = self.trace_evidence(b)?;

        let sources_a: BTreeSet<_> = trace_a.trace_steps.iter()
            .map(|s| s.from.clone()).collect();
        let sources_b: BTreeSet<_> = trace_b.trace_steps.iter()
            .map(|s| s.from.clone()).collect();

        let shared: Vec<_> = sources_a.intersection(&sources_b).cloned().collect();
        let only_a: Vec<_> = sources_a.difference(&sources_b).cloned().collect();
        let only_b: Vec<_> = sources_b.difference(&sources_a).cloned().collect();
        let mut conflicting = only_a;
        conflicting.extend(only_b);

        let is_conflicting_empty = conflicting.is_empty();
        Ok(ClaimComparison {
            claim_a: a.clone(),
            claim_b: b.clone(),
            shared_evidence: shared,
            conflicting_evidence: conflicting,
            supports_same_conclusion: is_conflicting_empty,
        })
    }

    /// Check reproduction status for a claim.
    pub fn reproduction_status(&self, claim_id: &NodeId) -> Result<ReproductionStatus, String> {
        let attempts: Vec<_> = self.edges.iter()
            .filter(|e| {
                &e.target == claim_id
                    && (e.relation == EdgeKind::Reproduces
                        || e.relation == EdgeKind::FailsToReproduce)
            })
            .map(|e| {
                let outcome = match e.relation {
                    EdgeKind::Reproduces => ReproductionOutcome::Reproduced,
                    EdgeKind::FailsToReproduce => ReproductionOutcome::FailedToReproduce,
                    _ => ReproductionOutcome::Inconclusive,
                };
                ReproductionAttempt {
                    run_id: e.run_id.clone(),
                    timestamp: e.timestamp.to_rfc3339(),
                    outcome,
                    evidence_node_id: e.source.clone(),
                }
            })
            .collect();

        let latest = attempts.last()
            .map(|a| a.outcome)
            .unwrap_or(ReproductionOutcome::Pending);

        Ok(ReproductionStatus {
            claim_node_id: claim_id.clone(),
            reproduction_attempts: attempts,
            latest_status: latest,
        })
    }
}

// ── Consistency checks (LS5-12) ────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistencyReport {
    pub graph_id: String,
    pub violations: Vec<Violation>,
    pub is_consistent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Violation {
    pub kind: ViolationKind,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViolationKind {
    DanglingNode,
    SelfReferencingClaim,
    CycleDetected,
    CrossProjectEdge,
    MissingArtifactCitation,
    ReviewerWithoutIdentity,
    SupersededArtifactAsCurrent,
}

impl EvidenceGraph {
    /// Run all consistency checks. Returns a report.
    pub fn check_consistency(&self) -> ConsistencyReport {
        let mut violations = Vec::new();

        // Dangling edges
        for edge in &self.edges {
            if !self.nodes.contains_key(&edge.source) {
                violations.push(Violation {
                    kind: ViolationKind::DanglingNode,
                    detail: format!("source {:?} not in graph", edge.source),
                });
            }
            if !self.nodes.contains_key(&edge.target) {
                violations.push(Violation {
                    kind: ViolationKind::DanglingNode,
                    detail: format!("target {:?} not in graph", edge.target),
                });
            }
        }

        // Self-referencing claims
        for node in self.nodes.values() {
            if matches!(node.kind, NodeKind::Claim) {
                for edge in &self.edges {
                    if edge.source == node.node_id && edge.target == node.node_id {
                        violations.push(Violation {
                            kind: ViolationKind::SelfReferencingClaim,
                            detail: format!("claim {:?} references itself", node.node_id),
                        });
                    }
                }
            }
        }

        // Reviewer verdicts must have identity
        for node in self.nodes.values() {
            if matches!(node.kind, NodeKind::ReviewerVerdict) {
                if node.created_by.is_empty() {
                    violations.push(Violation {
                        kind: ViolationKind::ReviewerWithoutIdentity,
                        detail: format!("reviewer verdict {:?} has no identity", node.node_id),
                    });
                }
            }
        }

        // Check for superseded artifacts used as current
        for edge in &self.edges {
            if edge.relation == EdgeKind::Supersedes {
                // The superseded artifact should not appear as a "Supports" source
                let superseded = &edge.target;
                let used_as_current = self.edges.iter().any(|e| {
                    &e.source == superseded
                        && e.relation == EdgeKind::Supports
                });
                if used_as_current {
                    violations.push(Violation {
                        kind: ViolationKind::SupersededArtifactAsCurrent,
                        detail: format!("{:?} is superseded but used as current evidence", superseded),
                    });
                }
            }
        }

        ConsistencyReport {
            graph_id: format!("graph-{}", self.project_id.0),
            is_consistent: violations.is_empty(),
            violations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::evidence_graph::EvidenceEdge;
    use super::super::model::ProjectId;
    use chrono::Utc;
    use std::collections::BTreeMap;

    fn build_test_graph() -> EvidenceGraph {
        let proj = ProjectId("test".into());
        let mut g = EvidenceGraph::new(proj.clone());
        let now = Utc::now();

        g.nodes.insert(NodeId("src".into()), EvidenceNode {
            node_id: NodeId("src".into()),
            kind: NodeKind::SourceArtifact,
            project_id: proj.clone(),
            label: "Source".into(),
            artifact_sha256: Some("sha256:src".into()),
            run_id: Some("r1".into()),
            created_by: "test".into(),
            created_at: now,
            metadata: BTreeMap::new(),
        });

        g.nodes.insert(NodeId("claim".into()), EvidenceNode {
            node_id: NodeId("claim".into()),
            kind: NodeKind::Claim,
            project_id: proj,
            label: "Claim".into(),
            artifact_sha256: None,
            run_id: Some("r1".into()),
            created_by: "researcher".into(),
            created_at: now,
            metadata: BTreeMap::new(),
        });

        g.edges.push(EvidenceEdge {
            source: NodeId("src".into()),
            target: NodeId("claim".into()),
            relation: EdgeKind::Supports,
            actor: "test".into(),
            timestamp: now,
            run_id: "r1".into(),
            supporting_artifact_sha256: "sha256:src".into(),
            confidence_kind: "high".into(),
        });

        g
    }

    #[test]
    fn trace_evidence_back_to_source() {
        let g = build_test_graph();
        let trace = g.trace_evidence(&NodeId("claim".into())).unwrap();
        assert!(!trace.trace_steps.is_empty());
        assert!(trace.depth > 0);
    }

    #[test]
    fn consistency_report_clean_graph() {
        let g = build_test_graph();
        let report = g.check_consistency();
        assert!(report.is_consistent);
        assert!(report.violations.is_empty());
    }

    #[test]
    fn consistency_detects_dangling_edge() {
        let mut g = build_test_graph();
        g.edges.push(EvidenceEdge {
            source: NodeId("ghost".into()),
            target: NodeId("src".into()),
            relation: EdgeKind::Supports,
            actor: "test".into(),
            timestamp: Utc::now(),
            run_id: "r1".into(),
            supporting_artifact_sha256: "sha256:x".into(),
            confidence_kind: "low".into(),
        });
        let report = g.check_consistency();
        assert!(!report.is_consistent);
    }
}
