pub mod claim;
pub mod evidence_graph;
pub mod model;

pub use claim::{Claim, ClaimStatus, Citation, CitationId};
pub use evidence_graph::{EdgeKind, EvidenceEdge, EvidenceGraph, EvidenceNode, NodeId, NodeKind};
pub use model::{Hypothesis, HypothesisStatus, OwnerId, ProjectId, ProjectStatus, ResearchProject};
