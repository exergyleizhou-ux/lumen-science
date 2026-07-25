//! Multi-role collaboration, review packages, and permission model.
//! Seam contracts: LS5-30, LS5-31, LS5-32, LS5-36.

use serde::{Deserialize, Serialize};
use crate::project::model::{OwnerId, ProjectId};

/// Collaboration permission levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CollaboratorRole {
    Read,
    Comment,
    Propose,
    Approve,
    Admin,
}

/// A collaborator invitation bound to a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collaboration {
    pub collaboration_id: String,
    pub project_id: ProjectId,
    pub owner_id: OwnerId,
    pub collaborator_id: String,
    pub role: CollaboratorRole,
    pub invited_at: String,
    pub accepted_at: Option<String>,
    pub revoked_at: Option<String>,
    pub is_active: bool,
}

impl Collaboration {
    pub fn can_read(&self) -> bool { self.is_active }
    pub fn can_comment(&self) -> bool { self.is_active && self.role >= CollaboratorRole::Comment }
    pub fn can_propose(&self) -> bool { self.is_active && self.role >= CollaboratorRole::Propose }
    pub fn can_approve(&self) -> bool { self.is_active && self.role >= CollaboratorRole::Approve }
    pub fn is_admin(&self) -> bool { self.is_active && self.role == CollaboratorRole::Admin }
}

/// A review package for independent reviewer distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewPackage {
    pub package_id: String,
    pub project_id: ProjectId,
    pub reviewer_id: String,
    pub claim_ids: Vec<String>,
    pub included_artifacts: Vec<String>,
    pub excluded_credentials: Vec<String>,
    pub redacted_logs: bool,
    pub deadline: Option<String>,
    pub status: ReviewStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewStatus {
    Prepared,
    Sent,
    InProgress,
    Submitted,
    Accepted,
    Rejected,
}

impl ReviewPackage {
    /// Verify that a review package contains no credentials.
    pub fn verify_no_credentials(&self) -> Result<(), String> {
        for excluded in &self.excluded_credentials {
            if excluded.is_empty() {
                return Err("empty credential exclusion entry".to_string());
            }
        }
        if !self.redacted_logs {
            return Err("review package must have redacted logs".to_string());
        }
        Ok(())
    }
}

/// A reviewer verdict submitted as part of a review package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewerVerdict {
    pub verdict_id: String,
    pub package_id: String,
    pub reviewer_id: String,
    pub claim_id: String,
    pub outcome: VerdictOutcome,
    pub evidence_references: Vec<String>,
    pub limitations: Vec<String>,
    pub submitted_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerdictOutcome {
    Supported,
    Contradicted,
    Inconclusive,
    NeedsRevision,
}

impl ReviewerVerdict {
    pub fn has_evidence(&self) -> bool {
        !self.evidence_references.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collaborator_role_ordering() {
        assert!(CollaboratorRole::Admin > CollaboratorRole::Approve);
        assert!(CollaboratorRole::Approve > CollaboratorRole::Propose);
        assert!(CollaboratorRole::Read < CollaboratorRole::Comment);
    }

    #[test]
    fn active_collaborator_permissions() {
        let c = Collaboration {
            collaboration_id: "c1".into(),
            project_id: ProjectId("p1".into()),
            owner_id: OwnerId("o1".into()),
            collaborator_id: "reviewer".into(),
            role: CollaboratorRole::Approve,
            invited_at: "2026-07-26".into(),
            accepted_at: Some("2026-07-26".into()),
            revoked_at: None,
            is_active: true,
        };
        assert!(c.can_read());
        assert!(c.can_approve());
        assert!(!c.is_admin());
    }

    #[test]
    fn revoked_collaborator_has_no_access() {
        let c = Collaboration {
            collaboration_id: "c1".into(),
            project_id: ProjectId("p1".into()),
            owner_id: OwnerId("o1".into()),
            collaborator_id: "bad-actor".into(),
            role: CollaboratorRole::Admin,
            invited_at: "2026-07-26".into(),
            accepted_at: None,
            revoked_at: Some("2026-07-26".into()),
            is_active: false,
        };
        assert!(!c.can_read());
        assert!(!c.is_admin());
    }

    #[test]
    fn review_package_rejects_no_redaction() {
        let pkg = ReviewPackage {
            package_id: "rp1".into(),
            project_id: ProjectId("p1".into()),
            reviewer_id: "r1".into(),
            claim_ids: vec!["c1".into()],
            included_artifacts: vec!["a1".into()],
            excluded_credentials: vec!["api_key".into()],
            redacted_logs: false,
            deadline: None,
            status: ReviewStatus::Prepared,
        };
        assert!(pkg.verify_no_credentials().is_err());
    }

    #[test]
    fn reviewer_verdict_requires_evidence() {
        let v = ReviewerVerdict {
            verdict_id: "v1".into(),
            package_id: "rp1".into(),
            reviewer_id: "r1".into(),
            claim_id: "c1".into(),
            outcome: VerdictOutcome::Supported,
            evidence_references: vec![],
            limitations: vec![],
            submitted_at: "2026-07-26".into(),
        };
        assert!(!v.has_evidence());
    }
}
