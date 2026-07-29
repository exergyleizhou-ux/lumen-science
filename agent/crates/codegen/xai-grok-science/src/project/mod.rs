pub(crate) mod capability;
pub mod claim;
pub mod evidence_graph;
pub mod migration;
pub mod model;
pub mod mutation;
pub mod preview_store;
pub mod queries_store;
pub mod query;
pub mod review_store;
pub mod store;
pub mod workflows_store;

#[cfg(test)]
mod e2e_tests;

pub use claim::{Citation, CitationId, Claim, ClaimStatus};
pub use evidence_graph::validate_sha256_hex;
pub use evidence_graph::{EdgeKind, EvidenceEdge, EvidenceGraph, EvidenceNode, NodeId, NodeKind};
pub use migration::{
    HashVerification, MigratedArtifact, MigrationAdmission, MigrationCommit, MigrationManifest,
    MigrationRecoveryGrant, MigrationResult, MigrationSnapshot, V1ToV2Migration,
    VerifiedMigrationBundle,
};
pub use model::{
    Hypothesis, HypothesisStatus, OwnerId, ProjectId, ProjectStatus, ResearchProject,
    validate_project_id,
};
pub use mutation::{MutationOutcome, MutationRequest, OperationRecord, ProjectMutation};
pub use query::{
    ClaimComparison, ConsistencyReport, EvidenceTrace, ReproductionStatus, Violation, ViolationKind,
};
pub use review_store::{
    ReviewAdmission, ReviewRecord, ReviewRecoveryGrant, ReviewVerdict, ReviewedArtifact,
};
pub use store::{ProjectBundle, ProjectRecoveryReport, ProjectStore, RegisteredArtifact};
