//! ResearchProject — the V2 aggregate root.
//!
//! A ResearchProject groups runs, evidence, claims, and workflows under
//! a single research objective. It is owned by SessionActor and provides
//! the durable identity for all V2+ science operations.
//!
//! Seam contract: LS5-6.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Unique project identifier. Stable across reopening.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(pub String);

pub fn validate_project_id(project_id: &str) -> crate::Result<()> {
    if project_id.is_empty()
        || project_id.len() > 128
        || !project_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(crate::ScienceError::Invalid(
            "projectId must be 1..=128 [A-Za-z0-9_-] characters".into(),
        ));
    }
    Ok(())
}

impl std::fmt::Display for ProjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique owner identifier. Bound to SessionActor's identity model.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OwnerId(pub String);

/// A research hypothesis within a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hypothesis {
    /// Unique id within the project.
    pub hypothesis_id: String,
    /// Human-readable statement.
    pub statement: String,
    /// Optional null hypothesis for statistical testing.
    pub null_hypothesis: Option<String>,
    /// Predictions derived from this hypothesis.
    pub predictions: Vec<String>,
    /// Status.
    pub status: HypothesisStatus,
    /// When created.
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HypothesisStatus {
    Proposed,
    Testing,
    Supported,
    Contradicted,
    Inconclusive,
    Superseded,
}

/// Project lifecycle state machine.
///
/// Draft → Planned → Active → ReviewPending → Accepted|Rejected|Inconclusive → Archived
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectStatus {
    /// Initial creation, not yet planned.
    Draft,
    /// Research plan defined, not yet executing.
    Planned,
    /// Active execution (runs, workflows in progress).
    Active,
    /// Execution complete, pending review.
    ReviewPending,
    /// Review complete — conclusions supported.
    Accepted,
    /// Review complete — conclusions not supported.
    Rejected,
    /// Review complete — results inconclusive.
    Inconclusive,
    /// Archived for long-term storage.
    Archived,
}

impl ProjectStatus {
    /// All valid transitions from this status.
    pub fn valid_transitions(&self) -> &[ProjectStatus] {
        match self {
            ProjectStatus::Draft => &[ProjectStatus::Planned],
            ProjectStatus::Planned => &[ProjectStatus::Active],
            ProjectStatus::Active => &[
                ProjectStatus::ReviewPending,
                ProjectStatus::Draft, // cancel back to planning
            ],
            ProjectStatus::ReviewPending => &[
                ProjectStatus::Accepted,
                ProjectStatus::Rejected,
                ProjectStatus::Inconclusive,
            ],
            ProjectStatus::Accepted
            | ProjectStatus::Rejected
            | ProjectStatus::Inconclusive => &[ProjectStatus::Archived],
            ProjectStatus::Archived => &[], // terminal
        }
    }

    /// Whether this state allows new runs/workflows.
    pub fn allows_execution(&self) -> bool {
        matches!(self, ProjectStatus::Active)
    }

    /// Whether this is a terminal research state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ProjectStatus::Accepted
                | ProjectStatus::Rejected
                | ProjectStatus::Inconclusive
                | ProjectStatus::Archived
        )
    }
}

/// The central aggregate for V2+ research.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchProject {
    /// Immutable project identity.
    pub project_id: ProjectId,
    /// Owning identity — single owner model (ADR-0505).
    pub owner_id: OwnerId,
    /// Human-readable title.
    pub title: String,
    /// The primary research question driving this project.
    pub research_question: String,
    /// Proposed hypotheses.
    pub hypotheses: Vec<Hypothesis>,
    /// Linked run IDs from V1/V2 sessions.
    pub sessions: Vec<String>,
    /// Linked dataset references.
    pub datasets: Vec<String>,
    /// Linked workflow IDs (V3+).
    pub workflows: Vec<String>,
    /// The evidence graph for this project.
    pub evidence_graph_id: Option<String>,
    /// Review policy name.
    pub review_policy: String,
    /// Data retention policy name.
    pub retention_policy: String,
    /// Current lifecycle status.
    pub status: ProjectStatus,
    /// Schema version for forward compatibility.
    pub schema_version: u32,
    /// When created.
    pub created_at: DateTime<Utc>,
    /// Last modification.
    pub updated_at: DateTime<Utc>,
}

impl ResearchProject {
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;

    /// Create a new draft project.
    pub fn new(
        project_id: ProjectId,
        owner_id: OwnerId,
        title: String,
        research_question: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            project_id,
            owner_id,
            title,
            research_question,
            hypotheses: vec![],
            sessions: vec![],
            datasets: vec![],
            workflows: vec![],
            evidence_graph_id: None,
            review_policy: "default".to_string(),
            retention_policy: "default".to_string(),
            status: ProjectStatus::Draft,
            schema_version: Self::CURRENT_SCHEMA_VERSION,
            created_at: now,
            updated_at: now,
        }
    }

    /// Transition to a new status. Returns Err if transition is invalid.
    pub fn transition_to(&mut self, new_status: ProjectStatus) -> Result<(), String> {
        if !self.status.valid_transitions().contains(&new_status) {
            return Err(format!(
                "invalid project transition: {:?} → {:?}",
                self.status, new_status
            ));
        }
        self.status = new_status;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Add a hypothesis. Only allowed in Draft/Planned status.
    pub fn add_hypothesis(&mut self, hypothesis: Hypothesis) -> Result<(), String> {
        if self.status != ProjectStatus::Draft && self.status != ProjectStatus::Planned {
            return Err("hypotheses can only be added in Draft or Planned".to_string());
        }
        self.hypotheses.push(hypothesis);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Link a session (run) to this project.
    pub fn add_session(&mut self, session_id: String) {
        if !self.sessions.contains(&session_id) {
            self.sessions.push(session_id);
            self.updated_at = Utc::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_project() -> ResearchProject {
        ResearchProject::new(
            ProjectId("test-proj-1".into()),
            OwnerId("test-owner".into()),
            "Test Project".into(),
            "What is the effect of X on Y?".into(),
        )
    }

    #[test]
    fn new_project_is_draft() {
        let p = test_project();
        assert_eq!(p.status, ProjectStatus::Draft);
        assert_eq!(p.schema_version, 1);
    }

    #[test]
    fn valid_transition_chain() {
        let mut p = test_project();
        assert!(p.transition_to(ProjectStatus::Planned).is_ok());
        assert!(p.transition_to(ProjectStatus::Active).is_ok());
        assert!(p.transition_to(ProjectStatus::ReviewPending).is_ok());
        assert!(p.transition_to(ProjectStatus::Accepted).is_ok());
        assert!(p.transition_to(ProjectStatus::Archived).is_ok());
    }

    #[test]
    fn rejects_invalid_transition() {
        let mut p = test_project();
        // Draft → Active is invalid (must go through Planned)
        assert!(p.transition_to(ProjectStatus::Active).is_err());
        // Draft → Accepted is invalid
        assert!(p.transition_to(ProjectStatus::Accepted).is_err());
    }

    #[test]
    fn hypothesis_only_in_draft_or_planned() {
        let mut p = test_project();
        let h = Hypothesis {
            hypothesis_id: "h1".into(),
            statement: "X increases Y".into(),
            null_hypothesis: Some("X has no effect on Y".into()),
            predictions: vec!["If X, then Y > baseline".into()],
            status: HypothesisStatus::Proposed,
            created_at: Utc::now(),
        };
        assert!(p.add_hypothesis(h.clone()).is_ok());

        p.transition_to(ProjectStatus::Planned).unwrap();
        assert!(p.add_hypothesis(h.clone()).is_ok());

        p.transition_to(ProjectStatus::Active).unwrap();
        assert!(p.add_hypothesis(h).is_err());
    }

    #[test]
    fn terminal_states_block_execution() {
        assert!(ProjectStatus::Accepted.is_terminal());
        assert!(ProjectStatus::Rejected.is_terminal());
        assert!(!ProjectStatus::Active.is_terminal());
        assert!(ProjectStatus::Active.allows_execution());
        assert!(!ProjectStatus::Archived.allows_execution());
    }

    #[test]
    fn session_deduplication() {
        let mut p = test_project();
        p.add_session("run-1".into());
        p.add_session("run-1".into());
        assert_eq!(p.sessions.len(), 1);
    }
}
