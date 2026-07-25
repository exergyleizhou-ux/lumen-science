//! Collaboration permission model and review package types.
//! Seam contracts: LS5-30, LS5-31, LS5-32.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::project::model::ProjectId;

// ── Permission Model (LS5-30, LS5-31) ──────────────────────────────

/// Permission levels for collaborators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CollaboratorRole {
    Read,
    Comment,
    Propose,
    Approve,
    Admin,
}

impl CollaboratorRole {
    /// Whether this role can modify project data.
    pub fn can_write(&self) -> bool { matches!(self, Self::Propose | Self::Approve | Self::Admin) }
    /// Whether this role can approve review verdicts.
    pub fn can_approve(&self) -> bool { matches!(self, Self::Approve | Self::Admin) }
    /// Whether this role can manage collaborators.
    pub fn can_manage(&self) -> bool { matches!(self, Self::Admin) }
}

/// A collaboration invitation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationInvite {
    pub invite_id: String,
    pub project_id: ProjectId,
    pub inviter_id: String,
    pub invitee_id: String,
    pub role: CollaboratorRole,
    pub status: InviteStatus,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InviteStatus { Pending, Accepted, Rejected, Expired, Revoked }

/// A collaborator record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collaborator {
    pub user_id: String,
    pub project_id: ProjectId,
    pub role: CollaboratorRole,
    pub joined_at: DateTime<Utc>,
}

/// Collaboration registry for a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationRegistry {
    pub project_id: ProjectId,
    pub owner_id: String,
    pub collaborators: Vec<Collaborator>,
    pub pending_invites: Vec<CollaborationInvite>,
}

impl CollaborationRegistry {
    pub fn new(project_id: ProjectId, owner_id: String) -> Self {
        Self { project_id, owner_id, collaborators: vec![], pending_invites: vec![] }
    }

    pub fn add_collaborator(&mut self, user_id: String, role: CollaboratorRole) -> Result<(), String> {
        if self.collaborators.iter().any(|c| c.user_id == user_id) {
            return Err(format!("user {} is already a collaborator", user_id));
        }
        self.collaborators.push(Collaborator { user_id, project_id: self.project_id.clone(), role, joined_at: Utc::now() });
        Ok(())
    }

    pub fn remove_collaborator(&mut self, user_id: &str) -> usize {
        let before = self.collaborators.len();
        self.collaborators.retain(|c| c.user_id != user_id);
        before - self.collaborators.len()
    }
}

// ── Review Package (LS5-32) ────────────────────────────────────────

/// A self-contained package for independent peer review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewPackage {
    pub package_id: String,
    pub project_id: ProjectId,
    pub reviewers: Vec<String>,
    pub artifact_ids: Vec<String>,
    pub evidence_graph_snapshot: Option<String>,
    pub created_at: DateTime<Utc>,
    pub status: ReviewPackageStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewPackageStatus { Draft, Sent, InReview, Completed, Expired }

// ── Multi-role review (LS5-33) ─────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewerRole {
    DomainExpert,
    Statistician,
    ReproducibilityReviewer,
    EthicsReviewer,
    PeerReviewer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewAssignment {
    pub assignment_id: String,
    pub reviewer_id: String,
    pub role: ReviewerRole,
    pub claim_ids: Vec<String>,
    pub deadline: Option<DateTime<Utc>>,
    pub completed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::model::ProjectId;

    #[test]
    fn collaborator_role_permissions() {
        assert!(!CollaboratorRole::Read.can_write());
        assert!(CollaboratorRole::Admin.can_write());
        assert!(CollaboratorRole::Admin.can_approve());
        assert!(CollaboratorRole::Admin.can_manage());
        assert!(!CollaboratorRole::Read.can_approve());
    }

    #[test]
    fn registry_rejects_duplicate_collaborator() {
        let proj = ProjectId("p1".into());
        let mut reg = CollaborationRegistry::new(proj, "owner".into());
        assert!(reg.add_collaborator("user1".into(), CollaboratorRole::Read).is_ok());
        assert!(reg.add_collaborator("user1".into(), CollaboratorRole::Read).is_err());
    }

    #[test]
    fn remove_collaborator_returns_count() {
        let proj = ProjectId("p1".into());
        let mut reg = CollaborationRegistry::new(proj, "owner".into());
        reg.add_collaborator("user1".into(), CollaboratorRole::Read).unwrap();
        reg.add_collaborator("user2".into(), CollaboratorRole::Comment).unwrap();
        assert_eq!(reg.remove_collaborator("user1"), 1);
        assert_eq!(reg.collaborators.len(), 1);
    }

    #[test]
    fn review_package_default_draft() {
        let pkg = ReviewPackage {
            package_id: "rp-1".into(), project_id: ProjectId("p1".into()),
            reviewers: vec!["r1".into()], artifact_ids: vec![],
            evidence_graph_snapshot: None, created_at: Utc::now(),
            status: ReviewPackageStatus::Draft,
        };
        assert_eq!(pkg.status, ReviewPackageStatus::Draft);
    }

    #[test]
    fn reviewer_role_completeness() {
        let roles = vec![
            ReviewerRole::DomainExpert, ReviewerRole::Statistician,
            ReviewerRole::ReproducibilityReviewer, ReviewerRole::EthicsReviewer,
            ReviewerRole::PeerReviewer,
        ];
        assert_eq!(roles.len(), 5);
    }
}
