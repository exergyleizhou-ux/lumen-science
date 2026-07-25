//! V1 → V2 data migration. Seam contract: LS5-13.
//!
//! Migrates V1 runs, artifacts, and evidence into a minimal ResearchProject.
//! Preserves all original hashes and replay compatibility.

use super::model::{ProjectId, ResearchProject};

/// Result of migrating a single V1 run into a V2 project.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MigrationResult {
    pub source_run_id: String,
    pub target_project_id: ProjectId,
    pub artifacts_migrated: usize,
    pub evidence_items_migrated: usize,
    pub hash_verification: HashVerification,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum HashVerification {
    /// All hashes verified byte-identical.
    Verified,
    /// Hash mismatch detected — migration aborted.
    Mismatch { expected: String, actual: String },
}

/// A minimal migration from a V1 run into a V2 project.
///
/// The migration:
/// 1. Creates a new ResearchProject from the run metadata
/// 2. Preserves all artifact SHA-256 values unchanged
/// 3. Maps V1 evidence into V2 evidence graph nodes
/// 4. Links the run as a session
/// 5. Verifies no hashes were altered
///
/// The caller (SessionActor) is responsible for durability and atomicity.
pub struct V1ToV2Migration;

impl V1ToV2Migration {
    /// Create a minimal project from a V1 run's metadata.
    pub fn create_project_from_run(
        project_id: ProjectId,
        owner_id: super::model::OwnerId,
        run_title: impl Into<String>,
        research_question: impl Into<String>,
        run_ids: Vec<String>,
    ) -> ResearchProject {
        let mut project = ResearchProject::new(
            project_id,
            owner_id,
            run_title.into(),
            research_question.into(),
        );
        for run_id in run_ids {
            project.add_session(run_id);
        }
        project
    }

    /// Verify that a V1 artifact's hash has been preserved during migration.
    pub fn verify_artifact_hash(
        original_sha256: &str,
        migrated_sha256: &str,
    ) -> HashVerification {
        if original_sha256 == migrated_sha256 {
            HashVerification::Verified
        } else {
            HashVerification::Mismatch {
                expected: original_sha256.to_string(),
                actual: migrated_sha256.to_string(),
            }
        }
    }

    /// Validate that a migration result has no hash mismatches.
    pub fn is_successful(result: &MigrationResult) -> bool {
        matches!(result.hash_verification, HashVerification::Verified)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::model::OwnerId;

    #[test]
    fn create_project_preserves_run_ids() {
        let project = V1ToV2Migration::create_project_from_run(
            ProjectId("p1".into()),
            OwnerId("owner".into()),
            "Test Migration",
            "What is X?",
            vec!["run-1".into(), "run-2".into()],
        );
        assert_eq!(project.sessions.len(), 2);
        assert!(project.sessions.contains(&"run-1".to_string()));
    }

    #[test]
    fn hash_verification_detects_mismatch() {
        let result = V1ToV2Migration::verify_artifact_hash("abc", "def");
        assert!(matches!(result, HashVerification::Mismatch { .. }));
        assert!(!V1ToV2Migration::is_successful(&MigrationResult {
            source_run_id: "r1".into(),
            target_project_id: ProjectId("p1".into()),
            artifacts_migrated: 1,
            evidence_items_migrated: 1,
            hash_verification: result,
        }));
    }

    #[test]
    fn hash_verification_passes_on_match() {
        let result = V1ToV2Migration::verify_artifact_hash("abc", "abc");
        assert!(matches!(result, HashVerification::Verified));
    }
}
