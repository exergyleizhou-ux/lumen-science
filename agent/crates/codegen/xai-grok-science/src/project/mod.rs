pub mod claim;
pub mod evidence_graph;
pub mod migration;
pub mod model;
pub mod query;

pub use claim::{Claim, ClaimStatus, Citation, CitationId};
pub use evidence_graph::{EdgeKind, EvidenceEdge, EvidenceGraph, EvidenceNode, NodeId, NodeKind};
pub use migration::{HashVerification, MigrationResult, V1ToV2Migration};
pub use model::{Hypothesis, HypothesisStatus, OwnerId, ProjectId, ProjectStatus, ResearchProject};
pub use query::{ClaimComparison, ConsistencyReport, EvidenceTrace, ReproductionStatus, Violation, ViolationKind};
