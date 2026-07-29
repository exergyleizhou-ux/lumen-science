//! Typed ecosystem capability admissions.
//!
//! A capability is **not** an independent executor. It maps an admitted external
//! tool descriptor (e.g. Biomni `query_uniprot`) onto an existing Lumen
//! SessionActor-gated method (`connector_fetch`) with a fixed connector id.
//!
//! Catalog total for Biomni tools remains 224; only capabilities listed in the
//! admission overlay are executable. Everything else stays quarantined.

pub mod biomni_uniprot;

pub use biomni_uniprot::{
    BIOMNI_QUERY_UNIPROT_CAPABILITY_ID, BIOMNI_QUERY_UNIPROT_PROVENANCE, BiomniUniprotInput,
    BiomniUniprotMappedFetch, map_biomni_query_uniprot, reject_unknown_capability,
};
