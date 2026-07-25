//! Protocol adapter trait and global registry. Seam contract: DS-1.
//!
//! Every connector implements [`ProtocolAdapter`] once and registers itself
//! in the global [`REGISTRY`]. The fetch pipeline and shell extension route
//! through the registry instead of per-connector `match` blocks. Unknown
//! connector IDs fail closed.
//!
//! The trait is responsible only for protocol-specific pure behavior:
//! building relative paths, counting expected exchanges, and parsing
//! responses. Permission, artifact, evidence, provenance, audit, and replay
//! remain in the Lumen kernel (SessionActor).

use super::ConnectorDescriptor;
use super::fetch::{FetchExchange, ParsedResponse};
use crate::Result;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Protocol-specific pure behavior contract for one external data connector.
pub trait ProtocolAdapter: Send + Sync {
    /// The compile-time descriptor for this connector (metadata, policy).
    fn descriptor(&self) -> &'static ConnectorDescriptor;

    /// Number of request/response exchanges this connector's v1 operation
    /// expects. Used by the product path to validate fixture sets before
    /// beginning a run.
    fn expected_exchanges(&self) -> usize;

    /// Build the relative-path sequence for a fixture-backed fetch. The
    /// caller wraps each path in [`ValidatedRequest`] via the kernel's
    /// policy gate; the adapter is responsible only for constructing the
    /// correct protocol-specific URLs.
    ///
    /// For pubmed, the esummary path depends on PMIDs parsed from the
    /// esearch fixture, so `fixtures` must already contain the raw
    /// response bytes for earlier exchanges.
    fn build_fixture_paths(
        &self,
        query: &str,
        max_results: u32,
        fixtures: &[Vec<u8>],
    ) -> Result<Vec<String>>;

    /// Parse one complete response-exchange sequence. The adapter must
    /// validate exchange count, detect malformed partial records, and
    /// fail closed before artifact registration.
    fn parse_responses(&self, exchanges: &[FetchExchange]) -> Result<ParsedResponse>;
}

/// Stable ordered registry of protocol adapters. A single global instance
/// lives in [`REGISTRY`]; adapters register at startup and unknown IDs
/// fail closed.
pub struct AdapterRegistry {
    adapters: Vec<Box<dyn ProtocolAdapter>>,
    by_id: HashMap<String, usize>,
}

impl AdapterRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        AdapterRegistry {
            adapters: Vec::new(),
            by_id: HashMap::new(),
        }
    }

    /// Register a protocol adapter. Returns an error on duplicate ID.
    /// Once registered the adapter's position in the registry is stable.
    pub fn register(&mut self, adapter: Box<dyn ProtocolAdapter>) -> std::result::Result<(), String> {
        let id = adapter.descriptor().id.to_owned();
        if self.by_id.contains_key(&id) {
            return Err(format!("duplicate adapter id: {id}"));
        }
        let index = self.adapters.len();
        self.adapters.push(adapter);
        self.by_id.insert(id, index);
        Ok(())
    }

    /// Look up an adapter by connector id. Returns `None` for unknown
    /// ids — the caller must fail closed.
    pub fn get(&self, id: &str) -> Option<&dyn ProtocolAdapter> {
        self.by_id.get(id).map(|&index| self.adapters[index].as_ref())
    }

    /// Convenience: expected exchange count for a connector, or `None`
    /// if the connector is unknown.
    pub fn expected_exchanges(&self, id: &str) -> Option<usize> {
        self.get(id).map(|a| a.expected_exchanges())
    }

    /// All registered adapters in stable registration order.
    pub fn all(&self) -> &[Box<dyn ProtocolAdapter>] {
        &self.adapters
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate that every descriptor in `descriptors` has a corresponding
/// adapter registered in `adapter_registry`, and that no adapter is
/// registered without a matching descriptor. Returns `Ok(())` when the two
/// sets cover exactly the same connector IDs in the same stable order.
pub fn validate_adapter_descriptor_coverage(
    adapter_registry: &AdapterRegistry,
    descriptors: &[ConnectorDescriptor],
) -> std::result::Result<(), String> {
    let adapter_ids: Vec<&str> = adapter_registry
        .all()
        .iter()
        .map(|a| a.descriptor().id)
        .collect();
    let descriptor_ids: Vec<&str> = descriptors.iter().map(|d| d.id).collect();

    // Check for missing adapters (descriptor with no matching adapter).
    for d in descriptors {
        if !adapter_registry.by_id.contains_key(d.id) {
            return Err(format!(
                "adapter coverage failure: descriptor '{}' has no registered adapter",
                d.id
            ));
        }
    }

    // Check for orphan adapters (adapter with no matching descriptor).
    for &id in &adapter_ids {
        if !descriptors.iter().any(|d| d.id == id) {
            return Err(format!(
                "adapter coverage failure: adapter '{}' has no matching descriptor",
                id
            ));
        }
    }

    // Enforce stable order match: adapter registration order must match
    // descriptor registry order.
    if adapter_ids != descriptor_ids {
        return Err(format!(
            "adapter/descriptor order mismatch: adapters {:?} != descriptors {:?}",
            adapter_ids, descriptor_ids
        ));
    }

    Ok(())
}

/// Validate the adapter registry against the descriptor registry.
/// Takes a reference to the local registry (NOT &REGISTRY) to avoid
/// recursive LazyLock deref deadlock during initialization.
fn validate_local_coverage(adapter_registry: &AdapterRegistry) {
    validate_adapter_descriptor_coverage(adapter_registry, super::registry())
        .expect("adapter/descriptor coverage validation failed at init");
}

/// Global protocol adapter registry. Initialized once at startup.
/// Adding a connector means implementing [`ProtocolAdapter`], adding a
/// descriptor to [`super::registry()`], and registering the adapter here.
/// Coverage validation runs at init and fails closed if any descriptor
/// is missing its adapter (or vice versa).
pub static REGISTRY: LazyLock<AdapterRegistry> = LazyLock::new(|| {
    let mut registry = AdapterRegistry::new();

    registry.register(Box::new(super::pubmed::PubmedAdapter)).expect("pubmed adapter");
    registry.register(Box::new(super::chembl::ChemblAdapter)).expect("chembl adapter");
    registry.register(Box::new(super::crossref::CrossrefAdapter)).expect("crossref adapter");
    registry.register(Box::new(super::uniprot::UniprotAdapter)).expect("uniprot adapter");
    registry.register(Box::new(super::europepmc::EuropepmcAdapter)).expect("europepmc adapter");
    registry.register(Box::new(super::openalex::OpenalexAdapter)).expect("openalex adapter");
    registry.register(Box::new(super::semantic_scholar::SemanticScholarAdapter)).expect("semantic-scholar adapter");
    registry.register(Box::new(super::arxiv::ArxivAdapter)).expect("arxiv adapter");
    registry.register(Box::new(super::biorxiv::BiorxivAdapter)).expect("biorxiv adapter");
    registry.register(Box::new(super::rcsb_pdb::RcsbPdbAdapter)).expect("rcsb-pdb adapter");
    registry.register(Box::new(super::pdbe::PdbeAdapter)).expect("pdbe adapter");
    registry.register(Box::new(super::alphafold::AlphafoldAdapter)).expect("alphafold adapter");
    registry.register(Box::new(super::interpro::InterproAdapter)).expect("interpro adapter");
    registry.register(Box::new(super::sifts::SiftsAdapter)).expect("sifts adapter");
    registry.register(Box::new(super::pubchem::PubchemAdapter)).expect("pubchem adapter");
    registry.register(Box::new(super::bindingdb::BindingdbAdapter)).expect("bindingdb adapter");
    registry.register(Box::new(super::gtopdb::GtopdbAdapter)).expect("gtopdb adapter");
    registry.register(Box::new(super::surechembl::SurechemblAdapter)).expect("surechembl adapter");
    registry.register(Box::new(super::chebi::ChebiAdapter)).expect("chebi adapter");
    registry.register(Box::new(super::ensembl::EnsemblAdapter)).expect("ensembl adapter");
    registry.register(Box::new(super::ncbi_gene::NcbiGeneAdapter)).expect("ncbi-gene adapter");
    registry.register(Box::new(super::dbsnp::DbsnpAdapter)).expect("dbsnp adapter");
    registry.register(Box::new(super::clinvar::ClinvarAdapter)).expect("clinvar adapter");
    registry.register(Box::new(super::gnomad::GnomadAdapter)).expect("gnomad adapter");
    registry.register(Box::new(super::ucsc::UcscAdapter)).expect("ucsc adapter");
    registry.register(Box::new(super::mygene::MygeneAdapter)).expect("mygene adapter");
    registry.register(Box::new(super::myvariant::MyvariantAdapter)).expect("myvariant adapter");
    registry.register(Box::new(super::reactome::ReactomeAdapter)).expect("reactome adapter");
    registry.register(Box::new(super::string_db::StringDbAdapter)).expect("string-db adapter");
    registry.register(Box::new(super::intact::IntactAdapter)).expect("intact adapter");
    registry.register(Box::new(super::wikipathways::WikpathwaysAdapter)).expect("wikipathways adapter");
    registry.register(Box::new(super::opentargets::OpentargetsAdapter)).expect("opentargets adapter");
    registry.register(Box::new(super::geo::GeoAdapter)).expect("geo adapter");
    registry.register(Box::new(super::arrayexpress::ArrayexpressAdapter)).expect("arrayexpress adapter");
    registry.register(Box::new(super::gtex::GtexAdapter)).expect("gtex adapter");
    registry.register(Box::new(super::hpa::HpaAdapter)).expect("hpa adapter");
    registry.register(Box::new(super::expression_atlas::ExpressionAtlasAdapter)).expect("expression-atlas adapter");
    registry.register(Box::new(super::single_cell_atlas::SingleCellAtlasAdapter)).expect("single-cell-atlas adapter");
    registry.register(Box::new(super::depmap::DepmapAdapter)).expect("depmap adapter");
    registry.register(Box::new(super::eutils::EutilsAdapter)).expect("eutils adapter");
    // BioGRID + KEGG intentionally NOT registered: rejected dispositions.
    // They remain in connectors::rejected_registry() for audit only.

    validate_local_coverage(&registry);

    registry
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_rejects_duplicate_id() {
        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(super::super::pubmed::PubmedAdapter)).unwrap();
        assert!(registry.register(Box::new(super::super::pubmed::PubmedAdapter)).is_err());
    }

    #[test]
    fn registry_unknown_id_returns_none() {
        assert!(REGISTRY.get("nonexistent").is_none());
        assert!(REGISTRY.expected_exchanges("nonexistent").is_none());
    }

    #[test]
    fn adapter_descriptor_coverage_is_one_to_one() {
        let descriptors = super::super::registry();
        let adapter_ids: Vec<&str> = REGISTRY.all().iter().map(|a| a.descriptor().id).collect();
        let descriptor_ids: Vec<&str> = descriptors.iter().map(|d| d.id).collect();
        assert_eq!(adapter_ids, descriptor_ids,
            "adapter/descriptor mismatch: adapters={adapter_ids:?} descriptors={descriptor_ids:?}");
    }

    #[test]
    fn coverage_rejects_missing_adapter() {
        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(super::super::pubmed::PubmedAdapter)).unwrap();
        let descriptors = super::super::registry();
        let result = validate_adapter_descriptor_coverage(&registry, descriptors);
        assert!(result.is_err(), "should reject when adapters are missing");
        let err = result.unwrap_err();
        assert!(err.contains("has no registered adapter"),
            "error must identify missing adapter, got: {err}");
        // The error must name a descriptor that is actually missing — not just
        // any descriptor.
        assert!(err.contains("chembl") || err.contains("crossref") || err.contains("uniprot")
            || err.contains("europepmc") || err.contains("openalex"),
            "error must name a specific missing descriptor");
    }

    /// A test-only descriptor whose ID deliberately does NOT exist in
    /// `connectors::registry()`. Used to verify that the coverage validator
    /// catches an adapter registered without a matching descriptor.
    static ORPHAN_DESCRIPTOR: ConnectorDescriptor = ConnectorDescriptor {
        id: "orphan-test-only",
        display_name: "Orphan (test-only)",
        auth_class: super::super::AuthClass::None,
        base_url: "https://example.invalid/",
        egress_hosts: &["example.invalid"],
        rate_limit: super::super::RateLimit { max_requests: 1, per_ms: 1000 },
        retry: super::super::RetryPolicy { max_attempts: 1, base_delay_ms: 100 },
        tos_url: "https://example.invalid/tos",
        user_notice: "orphan test only",
        data_class: super::super::DataClass::PublicReference,
        cache_policy: super::super::CachePolicy::NoStore,
        live_probe_path: "/test",
    };

    struct OrphanAdapter;

    impl ProtocolAdapter for OrphanAdapter {
        fn descriptor(&self) -> &'static ConnectorDescriptor { &ORPHAN_DESCRIPTOR }
        fn expected_exchanges(&self) -> usize { 1 }
        fn build_fixture_paths(&self, _query: &str, _max: u32, _fixtures: &[Vec<u8>]) -> crate::Result<Vec<String>> {
            Ok(vec!["/test".into()])
        }
        fn parse_responses(&self, _exchanges: &[crate::connectors::fetch::FetchExchange]) -> crate::Result<crate::connectors::fetch::ParsedResponse> {
            Ok(crate::connectors::fetch::ParsedResponse { total_hits: 0, records: vec![] })
        }
    }

    /// Helper: register every real adapter from the global REGISTRY into a
    /// local test registry so coverage tests operate on the full 42-adapter
    /// set, not a partial subset that drifts from descriptors.
    fn register_all_real_adapters(registry: &mut AdapterRegistry) {
        for adapter in REGISTRY.all() {
            let id = adapter.descriptor().id;
            let dup = registry.all().iter().any(|a| a.descriptor().id == id);
            if !dup {
                registry
                    .register(Box::new(TestAdapter {
                        id,
                        display_name: adapter.descriptor().display_name,
                        base_url: adapter.descriptor().base_url,
                        egress_hosts: adapter.descriptor().egress_hosts.to_vec(),
                        expected_exchanges: adapter.expected_exchanges(),
                    }))
                    .unwrap_or_else(|e| panic!("register {id}: {e}"));
            }
        }
    }

    /// Minimal adapter impl used by coverage tests to avoid pulling each
    /// connector module's full fixture/parse logic.
    struct TestAdapter {
        id: &'static str,
        display_name: &'static str,
        base_url: &'static str,
        egress_hosts: Vec<&'static str>,
        expected_exchanges: usize,
    }

    impl ProtocolAdapter for TestAdapter {
        fn descriptor(&self) -> &'static ConnectorDescriptor {
            // Leak a static descriptor — safe in test-only code.
            Box::leak(Box::new(ConnectorDescriptor {
                id: self.id,
                display_name: self.display_name,
                auth_class: super::super::AuthClass::None,
                base_url: self.base_url,
                egress_hosts: self.egress_hosts.clone().leak(),
                rate_limit: super::super::RateLimit { max_requests: 1, per_ms: 1_000 },
                retry: super::super::RetryPolicy { max_attempts: 1, base_delay_ms: 100 },
                tos_url: "https://example.com/tos",
                user_notice: "test only",
                data_class: super::super::DataClass::PublicReference,
                cache_policy: super::super::CachePolicy::NoStore,
                live_probe_path: "/test",
            }))
        }
        fn expected_exchanges(&self) -> usize { self.expected_exchanges }
        fn build_fixture_paths(&self, _q: &str, _m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> {
            Ok(vec!["/test".into()])
        }
        fn parse_responses(&self, _e: &[FetchExchange]) -> crate::Result<ParsedResponse> {
            Ok(ParsedResponse { total_hits: 0, records: vec![] })
        }
    }

    #[test]
    fn coverage_rejects_orphan_adapter() {
        let mut registry = AdapterRegistry::new();
        register_all_real_adapters(&mut registry);
        // Add one orphan whose descriptor is NOT in connectors::registry().
        registry.register(Box::new(OrphanAdapter)).unwrap();

        let descriptors = super::super::registry();
        let result = validate_adapter_descriptor_coverage(&registry, descriptors);
        assert!(result.is_err(), "should reject orphan adapters");
        let err = result.unwrap_err();
        assert!(
            err.contains("orphan-test-only"),
            "error must contain orphan adapter ID, got: {err}"
        );
        assert!(
            err.contains("has no matching descriptor"),
            "error must state 'no matching descriptor', got: {err}"
        );
    }

    #[test]
    fn every_adapter_descriptor_is_valid() {
        for adapter in REGISTRY.all() {
            super::super::validate_descriptor(adapter.descriptor())
                .unwrap_or_else(|e| panic!("{} invalid: {e}", adapter.descriptor().id));
        }
    }

    #[test]
    fn expected_exchanges_match_v1_protocol() {
        assert_eq!(REGISTRY.expected_exchanges("pubmed"), Some(2));
        assert_eq!(REGISTRY.expected_exchanges("chembl"), Some(1));
        assert_eq!(REGISTRY.expected_exchanges("crossref"), Some(1));
        assert_eq!(REGISTRY.expected_exchanges("uniprot"), Some(1));
        assert_eq!(REGISTRY.expected_exchanges("europepmc"), Some(1));
        assert_eq!(REGISTRY.expected_exchanges("openalex"), Some(1));
        assert_eq!(REGISTRY.expected_exchanges("semantic-scholar"), Some(1));
        assert_eq!(REGISTRY.expected_exchanges("arxiv"), Some(1));
        assert_eq!(REGISTRY.expected_exchanges("biorxiv"), Some(1));
        assert_eq!(REGISTRY.expected_exchanges("rcsb-pdb"), Some(1));
        assert_eq!(REGISTRY.expected_exchanges("pdbe"), Some(1));
        assert_eq!(REGISTRY.expected_exchanges("alphafold"), Some(1));
        assert_eq!(REGISTRY.expected_exchanges("interpro"), Some(1));
        assert_eq!(REGISTRY.expected_exchanges("sifts"), Some(1));
    }
}
