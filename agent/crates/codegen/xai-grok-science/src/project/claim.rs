//! Claim lifecycle and citation reference model.
//! Seam contracts: LS5-9, LS5-10.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::evidence_graph::{EdgeKind, EvidenceEdge, NodeId};
use super::model::ProjectId;

// ── Claim ──────────────────────────────────────────────────────────

/// Claim status state machine.
///
/// Proposed → EvidenceAttached → ReviewPending → Supported|Contradicted|Inconclusive → Superseded
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClaimStatus {
    /// Initial proposal, no evidence yet.
    Proposed,
    /// Evidence has been attached, awaiting review.
    EvidenceAttached,
    /// Under active review.
    ReviewPending,
    /// Review complete — evidence supports the claim.
    Supported,
    /// Review complete — evidence contradicts the claim.
    Contradicted,
    /// Review complete — results inconclusive.
    Inconclusive,
    /// A newer claim supersedes this one.
    Superseded,
}

impl ClaimStatus {
    pub fn valid_transitions(&self) -> &[ClaimStatus] {
        match self {
            ClaimStatus::Proposed => &[ClaimStatus::EvidenceAttached],
            ClaimStatus::EvidenceAttached => &[ClaimStatus::ReviewPending, ClaimStatus::Proposed],
            ClaimStatus::ReviewPending => &[
                ClaimStatus::Supported,
                ClaimStatus::Contradicted,
                ClaimStatus::Inconclusive,
            ],
            ClaimStatus::Supported
            | ClaimStatus::Contradicted
            | ClaimStatus::Inconclusive => &[ClaimStatus::Superseded],
            ClaimStatus::Superseded => &[], // terminal
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ClaimStatus::Supported
                | ClaimStatus::Contradicted
                | ClaimStatus::Inconclusive
                | ClaimStatus::Superseded
        )
    }
}

/// A research claim. Always bound to a project and must cite artifact evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub claim_id: String,
    pub project_id: ProjectId,
    /// Human-readable claim statement.
    pub statement: String,
    /// Current lifecycle status.
    pub status: ClaimStatus,
    /// Node in the evidence graph.
    pub evidence_node_id: Option<NodeId>,
    /// Limitations explicitly acknowledged by the claimant.
    pub limitations: Vec<String>,
    /// When created.
    pub created_at: DateTime<Utc>,
    /// Last status change.
    pub updated_at: DateTime<Utc>,
    /// Who proposed this claim.
    pub proposed_by: String,
}

impl Claim {
    pub fn new(
        claim_id: String,
        project_id: ProjectId,
        statement: String,
        proposed_by: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            claim_id,
            project_id,
            statement,
            status: ClaimStatus::Proposed,
            evidence_node_id: None,
            limitations: vec![],
            created_at: now,
            updated_at: now,
            proposed_by,
        }
    }

    pub fn transition_to(&mut self, new_status: ClaimStatus) -> Result<(), String> {
        if !self.status.valid_transitions().contains(&new_status) {
            return Err(format!(
                "invalid claim transition: {:?} → {:?}",
                self.status, new_status
            ));
        }
        self.status = new_status;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Attach evidence to this claim via a Supports edge.
    ///
    /// The digest must be canonical (64 lowercase hex); a truncated or
    /// differently-cased digest is not an artifact identity.
    pub fn build_support_edge(
        &self,
        target_node: NodeId,
        run_id: &str,
        artifact_sha256: &str,
    ) -> Result<EvidenceEdge, String> {
        super::evidence_graph::validate_sha256_hex(artifact_sha256)?;
        let source = self
            .evidence_node_id
            .clone()
            .ok_or("claim has no evidence node".to_string())?;
        Ok(EvidenceEdge {
            source,
            target: target_node,
            relation: EdgeKind::Supports,
            actor: self.proposed_by.clone(),
            timestamp: Utc::now(),
            run_id: run_id.to_string(),
            supporting_artifact_sha256: artifact_sha256.to_string(),
            confidence_kind: "high".to_string(),
        })
    }
}

// ── Citation ───────────────────────────────────────────────────────

/// Identifier types for external references.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CitationId {
    Doi(String),
    Pmid(String),
    Pmcid(String),
    ArxivId(String),
    Accession(String),
    Url(String),
}

/// A citation to an external scientific publication or dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub citation_id: String,
    pub id_type: CitationId,
    pub title: String,
    pub authors: Option<String>,
    pub year: Option<u32>,
    pub journal: Option<String>,
    pub dataset_doi: Option<String>,
    pub software_release: Option<String>,
    pub retrieval_timestamp: DateTime<Utc>,
    pub connector_source: Option<String>,
    pub raw_artifact_sha256: Option<String>,
    pub license: Option<String>,
    pub rights: Option<String>,
}

impl Citation {
    /// Create a minimal citation from a DOI.
    pub fn from_doi(doi: &str, title: &str) -> Self {
        Self {
            citation_id: format!("cite-{}", doi.replace('/', "-")),
            id_type: CitationId::Doi(doi.to_string()),
            title: title.to_string(),
            authors: None,
            year: None,
            journal: None,
            dataset_doi: None,
            software_release: None,
            retrieval_timestamp: Utc::now(),
            connector_source: None,
            raw_artifact_sha256: None,
            license: None,
            rights: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_lifecycle_full() {
        let mut c = Claim::new(
            "c1".into(),
            ProjectId("p1".into()),
            "X causes Y".into(),
            "researcher-1".into(),
        );
        assert_eq!(c.status, ClaimStatus::Proposed);

        c.transition_to(ClaimStatus::EvidenceAttached).unwrap();
        c.transition_to(ClaimStatus::ReviewPending).unwrap();
        c.transition_to(ClaimStatus::Supported).unwrap();
        c.transition_to(ClaimStatus::Superseded).unwrap();

        assert!(c.status.is_terminal());
    }

    #[test]
    fn claim_cannot_skip_review() {
        let mut c = Claim::new(
            "c1".into(),
            ProjectId("p1".into()),
            "X causes Y".into(),
            "r".into(),
        );
        // Proposed → Supported is invalid
        assert!(c.transition_to(ClaimStatus::Supported).is_err());
    }

    #[test]
    fn citation_from_doi() {
        let cit = Citation::from_doi("10.1234/example", "Example Paper");
        assert!(matches!(cit.id_type, CitationId::Doi(_)));
        assert_eq!(cit.title, "Example Paper");
    }
}
