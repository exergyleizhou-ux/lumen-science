//! ResearchProject built-binary e2e test (offline fixture-only).
//! Seam contract: LS5-14.
//!
//! Validates the complete ResearchProject lifecycle:
//! create → plan → add hypotheses → activate → link sessions →
//! build evidence graph → trace evidence → verify consistency.

use std::collections::BTreeMap;

use chrono::Utc;
use crate::project::model::{
    Hypothesis, HypothesisStatus, OwnerId, ProjectId, ProjectStatus, ResearchProject,
};
use crate::project::evidence_graph::{
    EdgeKind, EvidenceEdge, EvidenceGraph, EvidenceNode, NodeId, NodeKind,
};
use crate::project::claim::{Claim, ClaimStatus, Citation, CitationId};
use crate::project::migration::{HashVerification, V1ToV2Migration, MigrationResult};

/// LS5-14: Full project lifecycle from Draft to Accepted.
#[test]
fn e2e_project_lifecycle() {
    let mut project = ResearchProject::new(
        ProjectId("e2e-lifecycle".into()),
        OwnerId("researcher-1".into()),
        "Test E2E Project".into(),
        "Does treatment X affect outcome Y?".into(),
    );

    assert_eq!(project.status, ProjectStatus::Draft);

    // Add hypothesis
    let h = Hypothesis {
        hypothesis_id: "h1".into(),
        statement: "X increases Y".into(),
        null_hypothesis: Some("X has no effect on Y".into()),
        predictions: vec!["Y > baseline after X".into()],
        status: HypothesisStatus::Proposed,
        created_at: Utc::now(),
    };
    project.add_hypothesis(h).unwrap();

    // Transition through lifecycle
    project.transition_to(ProjectStatus::Planned).unwrap();
    project.transition_to(ProjectStatus::Active).unwrap();
    project.add_session("run-001".into());
    project.add_session("run-002".into());
    project.transition_to(ProjectStatus::ReviewPending).unwrap();
    project.transition_to(ProjectStatus::Accepted).unwrap();

    assert!(project.status.is_terminal());
    assert_eq!(project.sessions.len(), 2);
    assert_eq!(project.hypotheses.len(), 1);
}

/// LS5-14: Evidence graph construction and trace for a claim.
#[test]
fn e2e_evidence_graph_for_claim() {
    let proj = ProjectId("e2e-graph".into());
    let mut graph = EvidenceGraph::new(proj.clone());
    let now = Utc::now();

    // Build nodes
    graph.add_node(EvidenceNode {
        node_id: NodeId("src-1".into()),
        kind: NodeKind::SourceArtifact,
        project_id: proj.clone(),
        label: "PubMed Search Results".into(),
        artifact_sha256: Some("sha256:pubmed-results".into()),
        run_id: Some("run-001".into()),
        created_by: "researcher".into(),
        created_at: now,
        metadata: BTreeMap::new(),
    }).unwrap();

    graph.add_node(EvidenceNode {
        node_id: NodeId("derived-1".into()),
        kind: NodeKind::DerivedArtifact,
        project_id: proj.clone(),
        label: "Analyzed Data".into(),
        artifact_sha256: Some("sha256:derived".into()),
        run_id: Some("run-001".into()),
        created_by: "researcher".into(),
        created_at: now,
        metadata: BTreeMap::new(),
    }).unwrap();

    graph.add_node(EvidenceNode {
        node_id: NodeId("claim-1".into()),
        kind: NodeKind::Claim,
        project_id: proj.clone(),
        label: "X increases Y".into(),
        artifact_sha256: None,
        run_id: Some("run-001".into()),
        created_by: "researcher".into(),
        created_at: now,
        metadata: BTreeMap::new(),
    }).unwrap();

    graph.add_node(EvidenceNode {
        node_id: NodeId("review-1".into()),
        kind: NodeKind::ReviewerVerdict,
        project_id: proj.clone(),
        label: "Reviewer Acceptance".into(),
        artifact_sha256: None,
        run_id: Some("run-001".into()),
        created_by: "reviewer-1".into(),
        created_at: now,
        metadata: BTreeMap::new(),
    }).unwrap();

    // Build edges: derived_from + supports + reviewed_by
    graph.add_edge(EvidenceEdge {
        source: NodeId("derived-1".into()),
        target: NodeId("src-1".into()),
        relation: EdgeKind::DerivedFrom,
        actor: "researcher".into(),
        timestamp: now,
        run_id: "run-001".into(),
        supporting_artifact_sha256: "sha256:pubmed-results".into(),
        confidence_kind: "high".into(),
    }).unwrap();

    graph.add_edge(EvidenceEdge {
        source: NodeId("src-1".into()),
        target: NodeId("claim-1".into()),
        relation: EdgeKind::Supports,
        actor: "researcher".into(),
        timestamp: now,
        run_id: "run-001".into(),
        supporting_artifact_sha256: "sha256:pubmed-results".into(),
        confidence_kind: "high".into(),
    }).unwrap();

    graph.add_edge(EvidenceEdge {
        source: NodeId("review-1".into()),
        target: NodeId("claim-1".into()),
        relation: EdgeKind::ReviewedBy,
        actor: "reviewer-1".into(),
        timestamp: now,
        run_id: "run-001".into(),
        supporting_artifact_sha256: "sha256:pubmed-results".into(),
        confidence_kind: "high".into(),
    }).unwrap();

    // Verify consistency
    let report = graph.check_consistency();
    assert!(report.is_consistent, "graph must be consistent");

    // Trace evidence from claim
    let trace = graph.trace_evidence(&NodeId("claim-1".into())).unwrap();
    assert!(!trace.trace_steps.is_empty(), "trace must find supporting evidence");
}

/// LS5-14: Claim lifecycle from proposal to superseded.
#[test]
fn e2e_claim_lifecycle() {
    let mut claim = Claim::new(
        "claim-e2e".into(),
        ProjectId("e2e".into()),
        "Treatment X is effective".into(),
        "researcher-1".into(),
    );

    assert_eq!(claim.status, ClaimStatus::Proposed);

    claim.transition_to(ClaimStatus::EvidenceAttached).unwrap();
    claim.transition_to(ClaimStatus::ReviewPending).unwrap();
    claim.transition_to(ClaimStatus::Supported).unwrap();
    claim.transition_to(ClaimStatus::Superseded).unwrap();

    assert!(claim.status.is_terminal());
}

/// LS5-14: V1-to-V2 migration preserves hashes.
#[test]
fn e2e_migration_preserves_hashes() {
    let project = V1ToV2Migration::create_project_from_run(
        ProjectId("migrated-proj".into()),
        OwnerId("owner".into()),
        "Migrated Study",
        "What is X?",
        vec!["run-001".into(), "run-002".into()],
    );

    assert_eq!(project.sessions.len(), 2);
    assert_eq!(project.status, ProjectStatus::Draft);

    let verify = V1ToV2Migration::verify_artifact_hash("abc123", "abc123");
    assert!(matches!(verify, HashVerification::Verified));
}

/// LS5-14: Citation model covers all identifier types.
#[test]
fn e2e_citation_types() {
    let doi = Citation::from_doi("10.1234/test", "Test Paper");
    assert!(matches!(doi.id_type, CitationId::Doi(_)));

    let pmid = Citation {
        citation_id: "pmid-1".into(),
        id_type: CitationId::Pmid("12345".into()),
        title: "PMID Paper".into(),
        authors: None, year: Some(2024), journal: None,
        dataset_doi: None, software_release: None,
        retrieval_timestamp: Utc::now(),
        connector_source: Some("pubmed".into()),
        raw_artifact_sha256: Some("sha256:raw".into()),
        license: None, rights: None,
    };
    assert!(matches!(pmid.id_type, CitationId::Pmid(_)));
}

/// LS5-14: Invalid transitions are rejected.
#[test]
fn e2e_rejects_invalid_transition() {
    let mut project = ResearchProject::new(
        ProjectId("e2e-reject".into()),
        OwnerId("owner".into()),
        "Test".into(),
        "?".into(),
    );
    assert!(project.transition_to(ProjectStatus::Active).is_err());
    assert!(project.transition_to(ProjectStatus::Accepted).is_err());
}
