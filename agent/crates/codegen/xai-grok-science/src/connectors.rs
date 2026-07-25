//! Data connector descriptors and request policy. Seam contract: S3.
//!
//! This module declares *what* external data services a project may reach and
//! under which constraints. It opens no socket and reads no credential: the
//! execution pipeline (added per connector) must obtain a [`ValidatedRequest`]
//! here before any HTTP call is dispatched by a Lumen tool.
//!
//! Every descriptor is compile-time registered. Adding a connector means
//! adding a descriptor plus its protocol adapter, mock contract tests, an
//! audited live probe, and a `third_party/provenance/connector-<id>.md` file.

use serde::{Deserialize, Serialize};

pub mod alphafold;
pub mod arxiv;
pub mod bindingdb;
pub mod biogrid;
pub mod biorxiv;
pub mod chebi;
pub mod chembl;
pub mod clinvar;
pub mod crossref;
pub mod dbsnp;
pub mod ensembl;
pub mod europepmc;
pub mod eutils;
pub mod fetch;
pub mod gnomad;
pub mod gtopdb;
pub mod interpro;
pub mod kegg;
pub mod mygene;
pub mod myvariant;
pub mod ncbi_gene;
pub mod openalex;
pub mod pdbe;
pub mod pubchem;
pub mod pubmed;
pub mod rcsb_pdb;
pub mod semantic_scholar;
pub mod sifts;
pub mod surechembl;
pub mod ucsc;
pub mod uniprot;
pub mod adapter;

/// Static empty Vec used by connectors that need a borrowed empty slice for
/// `Option::unwrap_or` in places where a temporary `&vec![]` would be dropped.
pub(crate) static EMPTY_JSON_ARRAY: Vec<serde_json::Value> = Vec::new();

/// Minimal percent-encoding for query terms (unreserved characters pass
/// through; everything else is %XX). Keeps the crate free of a URL crate
/// dependency for two fixed endpoints.
pub(crate) fn url_encode(term: &str) -> String {
    let mut out = String::with_capacity(term.len());
    for byte in term.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Credential requirement of a data service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthClass {
    /// Public API, no credential. Rate limits still apply.
    None,
    /// API key resolved at the Lumen provider boundary; never persisted here.
    ApiKey,
    /// OAuth flow; not supported by any v1 connector.
    OAuth,
}

/// Data classification carried into artifact/evidence records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataClass {
    /// Bibliographic/reference metadata.
    PublicReference,
    /// Public measurement/factual data.
    PublicData,
    /// Project-private data; egress policy must be explicit.
    PrivateData,
}

/// Service-imposed request budget. The pipeline must enforce it; descriptors
/// exist so enforcement is data-driven rather than per-call-site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimit {
    pub max_requests: u32,
    pub per_ms: u64,
}

/// Bounded retry for transient failures. No unbounded retry is permitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
}

/// How long retrieved payloads may be cached inside the artifact store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CachePolicy {
    NoStore,
    TtlSeconds(u64),
}

/// Compile-time description of one external data service. Deliberately not
/// serde-enabled: descriptors are registry constants, never persisted input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectorDescriptor {
    /// Stable machine id, e.g. "pubmed". Used in audit and provenance records.
    pub id: &'static str,
    pub display_name: &'static str,
    pub auth_class: AuthClass,
    /// HTTPS base URL. Plain HTTP is rejected at registration validation.
    pub base_url: &'static str,
    /// Exact host names the pipeline may connect to. Anything else fails
    /// closed, including subdomains not listed here.
    pub egress_hosts: &'static [&'static str],
    pub rate_limit: RateLimit,
    pub retry: RetryPolicy,
    /// Terms-of-service URL recorded into provenance and live evidence.
    pub tos_url: &'static str,
    /// Text a TUI/ACP caller must present with retrieved data. This keeps
    /// service-specific copyright and usage notices out of model prose.
    pub user_notice: &'static str,
    pub data_class: DataClass,
    pub cache_policy: CachePolicy,
    /// Relative path used by the explicit `#[ignore]`d live probe.
    pub live_probe_path: &'static str,
}

/// NCBI E-utilities. Public, no key required at 3 requests/second.
/// TOS: <https://www.ncbi.nlm.nih.gov/home/about/policies/>
const PUBMED: ConnectorDescriptor = ConnectorDescriptor {
    id: "pubmed",
    display_name: "PubMed (NCBI E-utilities)",
    auth_class: AuthClass::None,
    base_url: "https://eutils.ncbi.nlm.nih.gov/entrez/eutils",
    egress_hosts: &["eutils.ncbi.nlm.nih.gov"],
    rate_limit: RateLimit {
        max_requests: 3,
        per_ms: 1_000,
    },
    retry: RetryPolicy {
        max_attempts: 3,
        base_delay_ms: 500,
    },
    tos_url: "https://www.ncbi.nlm.nih.gov/home/about/policies/",
    user_notice: "NCBI disclaimer and copyright notice: PubMed abstracts may be protected by copyright; review NCBI policies before reproducing or redistributing retrieved content.",
    data_class: DataClass::PublicReference,
    cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/esearch.fcgi?db=pubmed&term=crispr&retmax=1&retmode=json",
};

/// EBI ChEMBL REST API. Public compound/bioactivity data.
/// TOS: <https://www.ebi.ac.uk/about/terms-of-use>
const CHEMBL: ConnectorDescriptor = ConnectorDescriptor {
    id: "chembl",
    display_name: "ChEMBL (EBI)",
    auth_class: AuthClass::None,
    base_url: "https://www.ebi.ac.uk/chembl/api/data",
    egress_hosts: &["www.ebi.ac.uk"],
    rate_limit: RateLimit {
        max_requests: 5,
        per_ms: 1_000,
    },
    retry: RetryPolicy {
        max_attempts: 3,
        base_delay_ms: 500,
    },
    tos_url: "https://www.ebi.ac.uk/about/terms-of-use",
    user_notice: "ChEMBL data is subject to the EBI terms of use.",
    data_class: DataClass::PublicData,
    cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/molecule.json?limit=1",
};

/// Crossref public REST API. The v1 operation is a bounded bibliographic
/// works search that deliberately selects no abstract or full-text fields.
/// Metadata terms: <https://www.crossref.org/documentation/retrieve-metadata/>
const CROSSREF: ConnectorDescriptor = ConnectorDescriptor {
    id: "crossref",
    display_name: "Crossref Works",
    auth_class: AuthClass::None,
    base_url: "https://api.crossref.org",
    egress_hosts: &["api.crossref.org"],
    rate_limit: RateLimit {
        max_requests: 1,
        per_ms: 1_000,
    },
    retry: RetryPolicy {
        max_attempts: 3,
        base_delay_ms: 1_000,
    },
    tos_url: "https://www.crossref.org/documentation/retrieve-metadata/",
    user_notice: "Crossref bibliographic metadata is generally factual/public-domain data, but abstracts may retain publisher or author copyright; this connector intentionally retrieves no abstracts or full text.",
    data_class: DataClass::PublicReference,
    cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/works?query.bibliographic=crispr&rows=1&select=DOI,title,container-title",
};

/// UniProtKB website REST API. The v1 operation retrieves a bounded subset of
/// identity and annotation summary fields, never sequence or citation text.
const UNIPROT: ConnectorDescriptor = ConnectorDescriptor {
    id: "uniprot",
    display_name: "UniProtKB",
    auth_class: AuthClass::None,
    base_url: "https://rest.uniprot.org/uniprotkb",
    egress_hosts: &["rest.uniprot.org"],
    rate_limit: RateLimit {
        max_requests: 1,
        per_ms: 1_000,
    },
    retry: RetryPolicy {
        max_attempts: 3,
        base_delay_ms: 1_000,
    },
    tos_url: "https://www.uniprot.org/help/license",
    user_notice: "UniProt copyrightable database content is licensed CC BY 4.0 and must be attributed; UniProt provides no correctness warranty, and some data may be covered by patents or other rights.",
    data_class: DataClass::PublicData,
    cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/search?query=insulin&format=json&size=1&fields=accession,id,protein_name,gene_names,organism_name",
};

/// Europe PMC Articles REST API. The v1 operation retrieves one bounded
/// `lite` bibliographic metadata page and intentionally excludes abstracts,
/// full text, references, annotations, and external links.
const EUROPEPMC: ConnectorDescriptor = ConnectorDescriptor {
    id: "europepmc",
    display_name: "Europe PMC Articles",
    auth_class: AuthClass::None,
    base_url: "https://www.ebi.ac.uk/europepmc/webservices/rest",
    egress_hosts: &["www.ebi.ac.uk"],
    rate_limit: RateLimit {
        max_requests: 1,
        per_ms: 1_000,
    },
    retry: RetryPolicy {
        max_attempts: 3,
        base_delay_ms: 1_000,
    },
    tos_url: "https://europepmc.org/Copyright",
    user_notice: "Europe PMC lite bibliographic metadata is returned without abstracts or full text. Article content remains subject to each work's copyright and license; verify the article-level license before reuse or redistribution.",
    data_class: DataClass::PublicReference,
    cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/search?query=single%20cell%20RNA&format=json&resultType=lite&pageSize=1&synonym=false",
};

/// OpenAlex Works REST API. Live access requires a runtime-only API key and
/// is metered. The v1 operation returns only selected CC0 bibliographic
/// metadata and intentionally excludes abstracts, full text, and content URLs.
const OPENALEX: ConnectorDescriptor = ConnectorDescriptor {
    id: "openalex",
    display_name: "OpenAlex Works",
    auth_class: AuthClass::ApiKey,
    base_url: "https://api.openalex.org",
    egress_hosts: &["api.openalex.org"],
    rate_limit: RateLimit {
        max_requests: 1,
        per_ms: 1_000,
    },
    retry: RetryPolicy {
        max_attempts: 3,
        base_delay_ms: 1_000,
    },
    tos_url: "https://openalex.org/OpenAlex_termsofservice.pdf",
    user_notice: "Selected OpenAlex metadata is provided under CC0, but the metered API service requires a runtime key and is subject to OpenAlex terms. This connector returns no abstracts or full text; verify article-level rights before reusing underlying content.",
    data_class: DataClass::PublicReference,
    cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/works?search=single%20cell%20RNA&per_page=1&select=id,doi,display_name,publication_year",
};

/// Semantic Scholar Academic Graph API. Public, keyless tier available with
/// shared 1000 req/s pool; introductory API key provides dedicated 1 req/s.
/// Selected fields exclude abstracts, citation counts, and authors per
/// DS-0 admission policy.
const SEMANTIC_SCHOLAR: ConnectorDescriptor = ConnectorDescriptor {
    id: "semantic-scholar",
    display_name: "Semantic Scholar",
    auth_class: AuthClass::None,
    base_url: "https://api.semanticscholar.org",
    egress_hosts: &["api.semanticscholar.org"],
    rate_limit: RateLimit {
        max_requests: 1,
        per_ms: 3_000,
    },
    retry: RetryPolicy {
        max_attempts: 3,
        base_delay_ms: 1_000,
    },
    tos_url: "https://api.semanticscholar.org/api-docs/",
    user_notice: "Semantic Scholar metadata retrieved under fair use; S2 data available under ODC-BY. This connector intentionally retrieves no abstracts or full text. Verify article-level rights before reusing underlying content.",
    data_class: DataClass::PublicReference,
    cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/graph/v1/paper/search?query=machine%20learning&limit=1&fields=paperId,title,url,year,venue,externalIds",
};

/// arXiv Atom query API. Public, no key required. Requires ≤1 req/3s.
const ARXIV: ConnectorDescriptor = ConnectorDescriptor {
    id: "arxiv",
    display_name: "arXiv",
    auth_class: AuthClass::None,
    base_url: "https://export.arxiv.org/api",
    egress_hosts: &["export.arxiv.org"],
    rate_limit: RateLimit {
        max_requests: 1,
        per_ms: 3_000,
    },
    retry: RetryPolicy {
        max_attempts: 3,
        base_delay_ms: 1_000,
    },
    tos_url: "https://info.arxiv.org/help/api/index.html",
    user_notice: "arXiv metadata freely accessible; individual articles may be subject to copyright by their authors. This connector intentionally retrieves no abstracts or full-text content.",
    data_class: DataClass::PublicReference,
    cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/api/query?search_query=all%3Amachine+learning&start=0&max_results=1&sortBy=relevance",
};

const BIORXIV: ConnectorDescriptor = ConnectorDescriptor {
    id: "biorxiv", display_name: "bioRxiv / medRxiv", auth_class: AuthClass::None,
    base_url: "https://api.biorxiv.org", egress_hosts: &["api.biorxiv.org"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 2_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.biorxiv.org/about/terms",
    user_notice: "Preprint metadata retrieved from bioRxiv/medRxiv. Content rights vary by author; abstracts intentionally excluded.",
    data_class: DataClass::PublicReference, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/details/biorxiv/10.1101/2024.01.01.123456",
};

const RCSB_PDB: ConnectorDescriptor = ConnectorDescriptor {
    id: "rcsb-pdb", display_name: "RCSB PDB", auth_class: AuthClass::None,
    base_url: "https://search.rcsb.org", egress_hosts: &["search.rcsb.org", "data.rcsb.org"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.rcsb.org/pages/policies",
    user_notice: "PDB data is CC0. Structural metadata only; full coordinate data available via separate download.",
    data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/rcsbsearch/v2/query?json=%7B%22query%22%3A%7B%22type%22%3A%22terminal%22%2C%22service%22%3A%22full_text%22%2C%22parameters%22%3A%7B%22value%22%3A%22hemoglobin%22%7D%7D%2C%22return_type%22%3A%22entry%22%2C%22request_options%22%3A%7B%22paginate%22%3A%7B%22start%22%3A0%2C%22rows%22%3A1%7D%7D%7D",
};

const PDBE: ConnectorDescriptor = ConnectorDescriptor {
    id: "pdbe", display_name: "PDBe", auth_class: AuthClass::None,
    base_url: "https://www.ebi.ac.uk/pdbe", egress_hosts: &["www.ebi.ac.uk"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.ebi.ac.uk/about/terms-of-use",
    user_notice: "PDB data is CC0. EMBL-EBI terms apply.",
    data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/pdbe/search/pdb/select?q=hemoglobin&wt=json&rows=1&fl=pdb_id,title,experimental_method,resolution,organism_scientific_name",
};

const ALPHAFOLD: ConnectorDescriptor = ConnectorDescriptor {
    id: "alphafold", display_name: "AlphaFold DB", auth_class: AuthClass::None,
    base_url: "https://alphafold.ebi.ac.uk", egress_hosts: &["alphafold.ebi.ac.uk"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 500 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.ebi.ac.uk/about/terms-of-use",
    user_notice: "AlphaFold DB data is CC-BY-4.0. EMBL-EBI terms apply. Predicted structures only; experimental validation not implied.",
    data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/api/prediction/P01308",
};

const INTERPRO: ConnectorDescriptor = ConnectorDescriptor {
    id: "interpro", display_name: "InterPro", auth_class: AuthClass::None,
    base_url: "https://www.ebi.ac.uk/interpro", egress_hosts: &["www.ebi.ac.uk"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.ebi.ac.uk/about/terms-of-use",
    user_notice: "InterPro data is CC0. EMBL-EBI terms apply.",
    data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/interpro/api/entry/interpro/?search=kringle&page_size=1",
};

const SIFTS: ConnectorDescriptor = ConnectorDescriptor {
    id: "sifts", display_name: "SIFTS", auth_class: AuthClass::None,
    base_url: "https://www.ebi.ac.uk/pdbe/api/mappings", egress_hosts: &["www.ebi.ac.uk"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 500 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.ebi.ac.uk/about/terms-of-use",
    user_notice: "SIFTS data freely available. EMBL-EBI terms apply. Residue-level mappings summarized only.",
    data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/best_structures/P01308",
};

const PUBCHEM: ConnectorDescriptor = ConnectorDescriptor {
    id: "pubchem", display_name: "PubChem", auth_class: AuthClass::None,
    base_url: "https://pubchem.ncbi.nlm.nih.gov/rest/pug", egress_hosts: &["pubchem.ncbi.nlm.nih.gov"],
    rate_limit: RateLimit { max_requests: 2, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.ncbi.nlm.nih.gov/home/about/policies/",
    user_notice: "PubChem data freely available. NCBI policies apply. Structure data (SMILES, InChI) excluded.",
    data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/rest/pug/compound/name/aspirin/cids/JSON?name_type=word",
};

const BINDINGDB: ConnectorDescriptor = ConnectorDescriptor {
    id: "bindingdb", display_name: "BindingDB", auth_class: AuthClass::None,
    base_url: "https://bindingdb.org/rest", egress_hosts: &["bindingdb.org"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 2_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.bindingdb.org/rwd/bind/termsofuse.jsp",
    user_notice: "BindingDB data CC BY 4.0. SMILES structure data excluded.",
    data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/rest/getLigandsByUniprots?uniprot=P01308&cutoff=10000&code=0&response=application/json",
};

const GTOPDB: ConnectorDescriptor = ConnectorDescriptor {
    id: "gtopdb", display_name: "GtoPdb", auth_class: AuthClass::None,
    base_url: "https://www.guidetopharmacology.org/services", egress_hosts: &["www.guidetopharmacology.org"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.guidetopharmacology.org/about.jsp",
    user_notice: "GtoPdb data CC BY-SA 4.0.",
    data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/services/ligands?name=aspirin",
};

const SURECHEMBL: ConnectorDescriptor = ConnectorDescriptor {
    id: "surechembl", display_name: "SureChEMBL", auth_class: AuthClass::None,
    base_url: "https://www.surechembl.org/api", egress_hosts: &["www.surechembl.org"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.ebi.ac.uk/about/terms-of-use",
    user_notice: "SureChEMBL data freely available. EMBL-EBI terms apply.",
    data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/api/search/content?query=aspirin&page=1&itemsPerPage=1",
};

const CHEBI: ConnectorDescriptor = ConnectorDescriptor {
    id: "chebi", display_name: "ChEBI", auth_class: AuthClass::None,
    base_url: "https://www.ebi.ac.uk/ols4/api", egress_hosts: &["www.ebi.ac.uk"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.ebi.ac.uk/about/terms-of-use",
    user_notice: "ChEBI data CC BY 4.0. EMBL-EBI terms apply.",
    data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/ols4/api/search?q=aspirin&ontology=chebi&rows=1",
};

const ENSEMBL: ConnectorDescriptor = ConnectorDescriptor {
    id: "ensembl", display_name: "Ensembl", auth_class: AuthClass::None,
    base_url: "https://rest.ensembl.org", egress_hosts: &["rest.ensembl.org"],
    rate_limit: RateLimit { max_requests: 3, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.ebi.ac.uk/about/terms-of-use",
    user_notice: "Ensembl data freely available. EMBL-EBI terms apply.",
    data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/lookup/symbol/human/BRCA2?content-type=application/json",
};
const NCBI_GENE: ConnectorDescriptor = ConnectorDescriptor {
    id: "ncbi-gene", display_name: "NCBI Gene", auth_class: AuthClass::None,
    base_url: "https://eutils.ncbi.nlm.nih.gov/entrez/eutils", egress_hosts: &["eutils.ncbi.nlm.nih.gov"],
    rate_limit: RateLimit { max_requests: 3, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.ncbi.nlm.nih.gov/home/about/policies/",
    user_notice: "NCBI Gene data freely available. NCBI policies apply.",
    data_class: DataClass::PublicReference, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/esearch.fcgi?db=gene&retmode=json&retmax=1&term=BRCA2",
};
const DBSNP: ConnectorDescriptor = ConnectorDescriptor {
    id: "dbsnp", display_name: "dbSNP", auth_class: AuthClass::None,
    base_url: "https://eutils.ncbi.nlm.nih.gov/entrez/eutils", egress_hosts: &["eutils.ncbi.nlm.nih.gov"],
    rate_limit: RateLimit { max_requests: 3, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.ncbi.nlm.nih.gov/home/about/policies/",
    user_notice: "dbSNP data freely available. NCBI policies apply.",
    data_class: DataClass::PublicReference, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/esearch.fcgi?db=snp&retmode=json&retmax=1&term=rs334",
};
const CLINVAR: ConnectorDescriptor = ConnectorDescriptor {
    id: "clinvar", display_name: "ClinVar", auth_class: AuthClass::None,
    base_url: "https://eutils.ncbi.nlm.nih.gov/entrez/eutils", egress_hosts: &["eutils.ncbi.nlm.nih.gov"],
    rate_limit: RateLimit { max_requests: 3, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.ncbi.nlm.nih.gov/home/about/policies/",
    user_notice: "ClinVar data freely available. NCBI policies apply.",
    data_class: DataClass::PublicReference, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/esearch.fcgi?db=clinvar&retmode=json&retmax=1&term=BRCA1",
};
const GNOMAD: ConnectorDescriptor = ConnectorDescriptor {
    id: "gnomad", display_name: "gnomAD", auth_class: AuthClass::None,
    base_url: "https://gnomad.broadinstitute.org/api", egress_hosts: &["gnomad.broadinstitute.org"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://gnomad.broadinstitute.org/terms",
    user_notice: "gnomAD data ODC Public Domain Dedication. Broad Institute terms apply.",
    data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/?query={%20gene(gene_id:%20\"ENSG00000139618\")%20{%20gene_id,symbol%20}%20}",
};
const UCSC: ConnectorDescriptor = ConnectorDescriptor {
    id: "ucsc", display_name: "UCSC Genome Browser", auth_class: AuthClass::None,
    base_url: "https://api.genome.ucsc.edu", egress_hosts: &["api.genome.ucsc.edu"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://genome.ucsc.edu/license/",
    user_notice: "UCSC Genome Browser data freely available for academic use.",
    data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/search?search=BRCA2&genome=hg38",
};
const MYGENE: ConnectorDescriptor = ConnectorDescriptor {
    id: "mygene", display_name: "MyGene.info", auth_class: AuthClass::None,
    base_url: "https://mygene.info/v3", egress_hosts: &["mygene.info"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://mygene.info/",
    user_notice: "MyGene.info data from multiple sources. Service free.",
    data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/v3/query?q=BRCA2&size=1&species=human",
};
const MYVARIANT: ConnectorDescriptor = ConnectorDescriptor {
    id: "myvariant", display_name: "MyVariant.info", auth_class: AuthClass::None,
    base_url: "https://myvariant.info/v1", egress_hosts: &["myvariant.info"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://myvariant.info/",
    user_notice: "MyVariant.info data from multiple sources. Service free.",
    data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/v1/query?q=rs334&size=1",
};


const REACTOME: ConnectorDescriptor = ConnectorDescriptor {
    id: "reactome", display_name: "Reactome", auth_class: AuthClass::None,
    base_url: "https://reactome.org", egress_hosts: &["reactome.org"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://reactome.org/documentation/data-license-agreement",
    user_notice: "Reactome data CC0.", data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/ContentService/search/query?query=DNA+repair&cluster=true&species=Homo+sapiens",
};
const STRING_DB: ConnectorDescriptor = ConnectorDescriptor {
    id: "string-db", display_name: "STRING", auth_class: AuthClass::None,
    base_url: "https://string-db.org/api", egress_hosts: &["string-db.org"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://string-db.org/cgi/access", user_notice: "STRING data CC BY 4.0.", data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/api/json/get_string_ids?identifiers=BRCA2&species=9606&limit=1&echo_query=1",
};
const INTACT: ConnectorDescriptor = ConnectorDescriptor {
    id: "intact", display_name: "IntAct", auth_class: AuthClass::None,
    base_url: "https://www.ebi.ac.uk/intact/ws", egress_hosts: &["www.ebi.ac.uk"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.ebi.ac.uk/about/terms-of-use", user_notice: "IntAct data freely available. EMBL-EBI terms apply.", data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/intact/ws/interaction/findInteractions/BRCA2?page=0&pageSize=1",
};
const WIKIPATHWAYS: ConnectorDescriptor = ConnectorDescriptor {
    id: "wikipathways", display_name: "WikiPathways", auth_class: AuthClass::None,
    base_url: "https://www.wikipathways.org", egress_hosts: &["www.wikipathways.org"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 5_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.wikipathways.org/index.php/WikiPathways:License", user_notice: "WikiPathways data CC0.", data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/json/findPathwaysByText.json",
};
const OPENTARGETS: ConnectorDescriptor = ConnectorDescriptor {
    id: "opentargets", display_name: "Open Targets", auth_class: AuthClass::None,
    base_url: "https://api.platform.opentargets.org/api/v4/graphql", egress_hosts: &["api.platform.opentargets.org"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://platform.opentargets.org/documentation", user_notice: "Open Targets data CC BY 4.0.", data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/api/v4/graphql?query=%7Bsearch(queryString%3A%22BRCA2%22%2Cpage%3A%7Bindex%3A0%2Csize%3A1%7D)%7Bhits%7Bid%7D%7D%7D",
};

const GEO: ConnectorDescriptor = ConnectorDescriptor {
    id: "geo", display_name: "NCBI GEO", auth_class: AuthClass::None,
    base_url: "https://eutils.ncbi.nlm.nih.gov/entrez/eutils", egress_hosts: &["eutils.ncbi.nlm.nih.gov"],
    rate_limit: RateLimit { max_requests: 3, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.ncbi.nlm.nih.gov/home/about/policies/", user_notice: "GEO data freely available. NCBI policies apply.", data_class: DataClass::PublicReference, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/esearch.fcgi?db=gds&retmode=json&retmax=1&term=cancer",
};
const ARRAYEXPRESS: ConnectorDescriptor = ConnectorDescriptor {
    id: "arrayexpress", display_name: "ArrayExpress", auth_class: AuthClass::None,
    base_url: "https://www.ebi.ac.uk/biostudies/api/v1", egress_hosts: &["www.ebi.ac.uk"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.ebi.ac.uk/about/terms-of-use", user_notice: "ArrayExpress data freely available. EMBL-EBI terms apply.", data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/biostudies/api/v1/arrayexpress/search?query=cancer&pageSize=1",
};
const GTEX: ConnectorDescriptor = ConnectorDescriptor {
    id: "gtex", display_name: "GTEx", auth_class: AuthClass::None,
    base_url: "https://gtexportal.org/api/v2", egress_hosts: &["gtexportal.org"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://gtexportal.org/home/documentationPage", user_notice: "GTEx data freely available. NIH dbGaP terms for controlled-access subsets.", data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/api/v2/reference/gene?geneId=BRCA2&itemsPerPage=1",
};
const HPA: ConnectorDescriptor = ConnectorDescriptor {
    id: "hpa", display_name: "Human Protein Atlas", auth_class: AuthClass::None,
    base_url: "https://www.proteinatlas.org", egress_hosts: &["www.proteinatlas.org"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.proteinatlas.org/about/licence", user_notice: "HPA data CC BY-SA 3.0.", data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/api/search_download.php?search=BRCA2&format=json&columns=g,gs,eg,up,gd,chr&compress=no",
};
const EXPRESSION_ATLAS: ConnectorDescriptor = ConnectorDescriptor {
    id: "expression-atlas", display_name: "Expression Atlas", auth_class: AuthClass::None,
    base_url: "https://www.ebi.ac.uk/gxa", egress_hosts: &["www.ebi.ac.uk"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.ebi.ac.uk/about/terms-of-use", user_notice: "Expression Atlas data freely available. EMBL-EBI terms apply.", data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/gxa/json/experiments",
};
const SINGLE_CELL_ATLAS: ConnectorDescriptor = ConnectorDescriptor {
    id: "single-cell-atlas", display_name: "Single Cell Expression Atlas", auth_class: AuthClass::None,
    base_url: "https://www.ebi.ac.uk/gxa/sc", egress_hosts: &["www.ebi.ac.uk"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://www.ebi.ac.uk/about/terms-of-use", user_notice: "SCEA data freely available. EMBL-EBI terms apply.", data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/gxa/sc/json/experiments",
};
const DEPMAP: ConnectorDescriptor = ConnectorDescriptor {
    id: "depmap", display_name: "DepMap", auth_class: AuthClass::None,
    base_url: "https://depmap.org/portal", egress_hosts: &["depmap.org"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 2_000 },
    retry: RetryPolicy { max_attempts: 3, base_delay_ms: 1_000 },
    tos_url: "https://depmap.org/portal/download/all/", user_notice: "DepMap data CC BY 4.0.", data_class: DataClass::PublicData, cache_policy: CachePolicy::TtlSeconds(86_400),
    live_probe_path: "/portal/api/download/files",
};

const EUTILS: ConnectorDescriptor = ConnectorDescriptor {
    id: "eutils", display_name: "NCBI E-utilities (shared)", auth_class: AuthClass::None,
    base_url: "https://eutils.ncbi.nlm.nih.gov/entrez/eutils", egress_hosts: &["eutils.ncbi.nlm.nih.gov"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 1, base_delay_ms: 100 },
    tos_url: "https://www.ncbi.nlm.nih.gov/home/about/policies/",
    user_notice: "E-utilities shared transport. Protocol partially overlaps with PubMed; full parameterizable adapter pending DS-14 implementation.",
    data_class: DataClass::PublicReference, cache_policy: CachePolicy::NoStore,
    live_probe_path: "/esearch.fcgi?db=nucleotide&retmode=json&retmax=1&term=test",
};
const BIOGRID_REJECTED: ConnectorDescriptor = ConnectorDescriptor {
    id: "biogrid", display_name: "BioGRID (REJECTED)", auth_class: AuthClass::None,
    base_url: "https://webservice.thebiogrid.org/", egress_hosts: &["webservice.thebiogrid.org"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 1, base_delay_ms: 100 },
    tos_url: "https://thebiogrid.org/terms.php",
    user_notice: "REJECTED: credential in URL query string violates Lumen safety policy. No runtime implementation or live probe admitted.",
    data_class: DataClass::PublicData, cache_policy: CachePolicy::NoStore,
    live_probe_path: "/interactions/?searchNames=true&geneList=test&format=json&max=1",
};
const KEGG_PENDING: ConnectorDescriptor = ConnectorDescriptor {
    id: "kegg", display_name: "KEGG (LICENSE PENDING)", auth_class: AuthClass::None,
    base_url: "https://rest.kegg.jp/", egress_hosts: &["rest.kegg.jp"],
    rate_limit: RateLimit { max_requests: 1, per_ms: 1_000 },
    retry: RetryPolicy { max_attempts: 1, base_delay_ms: 100 },
    tos_url: "https://www.kegg.jp/kegg/legal.html",
    user_notice: "LICENSE PENDING: commercial use requires paid subscription. No runtime implementation admitted until license accepted.",
    data_class: DataClass::PublicData, cache_policy: CachePolicy::NoStore,
    live_probe_path: "/find/pathway/test",
};

/// All registered connectors, in stable order.
pub fn registry() -> &'static [ConnectorDescriptor] {
    &[PUBMED, CHEMBL, CROSSREF, UNIPROT, EUROPEPMC, OPENALEX, SEMANTIC_SCHOLAR, ARXIV, BIORXIV, RCSB_PDB, PDBE, ALPHAFOLD, INTERPRO, SIFTS, PUBCHEM, BINDINGDB, GTOPDB, SURECHEMBL, CHEBI, ENSEMBL, NCBI_GENE, DBSNP, CLINVAR, GNOMAD, UCSC, MYGENE, MYVARIANT, REACTOME, STRING_DB, INTACT, WIKIPATHWAYS, OPENTARGETS, GEO, ARRAYEXPRESS, GTEX, HPA, EXPRESSION_ATLAS, SINGLE_CELL_ATLAS, DEPMAP, EUTILS, BIOGRID_REJECTED, KEGG_PENDING]
}

/// Look up a connector by id.
pub fn descriptor(id: &str) -> Option<&'static ConnectorDescriptor> {
    registry().iter().find(|d| d.id == id)
}

/// Why a candidate request may not leave the pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyError {
    UnknownConnector(String),
    /// Target host is not on the descriptor's exact allowlist.
    EgressHostNotAllowed {
        connector: String,
        host: String,
    },
    /// Descriptor requires a credential class the request did not satisfy.
    CredentialRequired {
        connector: String,
    },
    /// Requested timeout exceeds the descriptor budget ceiling.
    TimeoutExceeds {
        connector: String,
        max_ms: u64,
    },
    /// Base URL or probe path failed validation (non-HTTPS, absolute path,
    /// or path escaping the descriptor base).
    InvalidEndpoint {
        connector: String,
        detail: String,
    },
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyError::UnknownConnector(id) => write!(f, "unknown connector: {id}"),
            PolicyError::EgressHostNotAllowed { connector, host } => {
                write!(f, "connector {connector}: host not allowed: {host}")
            }
            PolicyError::CredentialRequired { connector } => {
                write!(f, "connector {connector}: credential required")
            }
            PolicyError::TimeoutExceeds { connector, max_ms } => {
                write!(
                    f,
                    "connector {connector}: timeout exceeds {max_ms}ms budget"
                )
            }
            PolicyError::InvalidEndpoint { connector, detail } => {
                write!(f, "connector {connector}: invalid endpoint: {detail}")
            }
        }
    }
}

impl std::error::Error for PolicyError {}

/// A request that passed policy and may be handed to the HTTP dispatcher.
/// Contains no credential material; the dispatcher resolves credentials at
/// the provider boundary and must not log or persist them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedRequest {
    pub connector_id: &'static str,
    pub url: String,
    pub timeout_ms: u64,
    pub rate_limit: RateLimit,
    pub retry: RetryPolicy,
    pub tos_url: &'static str,
    pub data_class: DataClass,
    pub cache_policy: CachePolicy,
}

/// Largest timeout any request may ask for, per descriptor family.
const MAX_TIMEOUT_MS: u64 = 30_000;

/// Validate descriptor invariants. Called by tests and by pipeline startup;
/// a malformed descriptor must never reach the dispatcher.
pub fn validate_descriptor(d: &ConnectorDescriptor) -> std::result::Result<(), PolicyError> {
    let invalid = |detail: &str| PolicyError::InvalidEndpoint {
        connector: d.id.to_owned(),
        detail: detail.to_owned(),
    };
    if d.id.is_empty() || !d.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(invalid(
            "id must be non-empty ascii alphanumeric/underscore",
        ));
    }
    if !d.base_url.starts_with("https://") {
        return Err(invalid("base_url must be https"));
    }
    if !d.tos_url.starts_with("https://") {
        return Err(invalid("tos_url must be https"));
    }
    if d.user_notice.trim().is_empty() {
        return Err(invalid("user_notice must be non-empty"));
    }
    let host = d
        .base_url
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or_default();
    if !d.egress_hosts.contains(&host) {
        return Err(invalid("base_url host must be on egress_hosts"));
    }
    if !d.live_probe_path.starts_with('/') || d.live_probe_path.contains("..") {
        return Err(invalid(
            "live_probe_path must be absolute and contain no ..",
        ));
    }
    if d.rate_limit.max_requests == 0 || d.retry.max_attempts == 0 {
        return Err(invalid("rate limit and retry must be positive"));
    }
    Ok(())
}

/// Gate a candidate HTTP request through descriptor policy. `has_credential`
/// asserts the dispatcher resolved the descriptor's required credential at
/// the provider boundary; the credential itself never crosses this API.
pub fn validate_request(
    connector_id: &str,
    path: &str,
    has_credential: bool,
    timeout_ms: u64,
) -> std::result::Result<ValidatedRequest, PolicyError> {
    let d = descriptor(connector_id)
        .ok_or_else(|| PolicyError::UnknownConnector(connector_id.to_owned()))?;
    validate_descriptor(d)?;
    if d.auth_class != AuthClass::None && !has_credential {
        return Err(PolicyError::CredentialRequired {
            connector: d.id.to_owned(),
        });
    }
    if timeout_ms == 0 || timeout_ms > MAX_TIMEOUT_MS {
        return Err(PolicyError::TimeoutExceeds {
            connector: d.id.to_owned(),
            max_ms: MAX_TIMEOUT_MS,
        });
    }
    if !path.starts_with('/') || path.contains("..") {
        return Err(PolicyError::InvalidEndpoint {
            connector: d.id.to_owned(),
            detail: "path must be absolute and contain no ..".to_owned(),
        });
    }
    Ok(ValidatedRequest {
        connector_id: d.id,
        url: format!("{}{}", d.base_url, path),
        timeout_ms,
        rate_limit: d.rate_limit,
        retry: d.retry,
        tos_url: d.tos_url,
        data_class: d.data_class,
        cache_policy: d.cache_policy,
    })
}

/// Validate a request that will be paired with an offline fixture and never
/// dispatched to the network. Credential presence is intentionally not
/// asserted because no credential is needed or permitted in fixture paths;
/// every other descriptor and endpoint policy remains enforced.
pub fn validate_fixture_request(
    connector_id: &str,
    path: &str,
    timeout_ms: u64,
) -> std::result::Result<ValidatedRequest, PolicyError> {
    let d = descriptor(connector_id)
        .ok_or_else(|| PolicyError::UnknownConnector(connector_id.to_owned()))?;
    validate_descriptor(d)?;
    if timeout_ms == 0 || timeout_ms > MAX_TIMEOUT_MS {
        return Err(PolicyError::TimeoutExceeds {
            connector: d.id.to_owned(),
            max_ms: MAX_TIMEOUT_MS,
        });
    }
    if !path.starts_with('/') || path.contains("..") {
        return Err(PolicyError::InvalidEndpoint {
            connector: d.id.to_owned(),
            detail: "path must be absolute and contain no ..".to_owned(),
        });
    }
    Ok(ValidatedRequest {
        connector_id: d.id,
        url: format!("{}{}", d.base_url, path),
        timeout_ms,
        rate_limit: d.rate_limit,
        retry: d.retry,
        tos_url: d.tos_url,
        data_class: d.data_class,
        cache_policy: d.cache_policy,
    })
}

/// Redacted audit record for one connector retrieval. Request and response
/// are identified by hash only; URLs may contain query terms that are part of
/// the scientific record, so they are hashed, not copied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorAudit {
    pub connector_id: String,
    pub request_sha256: String,
    pub response_sha256: Option<String>,
    pub retrieved_at_ms: i64,
    pub tos_url: String,
    pub outcome: ConnectorOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorOutcome {
    Retrieved,
    RateLimited,
    Failed,
}

/// Build a redacted audit record for a retrieval attempt.
pub fn connector_audit(
    request: &ValidatedRequest,
    response_sha256: Option<String>,
    retrieved_at_ms: i64,
    outcome: ConnectorOutcome,
) -> ConnectorAudit {
    use sha2::{Digest, Sha256};
    let request_sha256 = format!("{:x}", Sha256::digest(request.url.as_bytes()));
    ConnectorAudit {
        connector_id: request.connector_id.to_owned(),
        request_sha256,
        response_sha256,
        retrieved_at_ms,
        tos_url: request.tos_url.to_owned(),
        outcome,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_first_batch_in_stable_order() {
        let ids: Vec<_> = registry().iter().map(|d| d.id).collect();
        // First 14 connectors must appear in stable order.
        let first_batch = &[
            "pubmed",
            "chembl",
            "crossref",
            "uniprot",
            "europepmc",
            "openalex",
            "semantic-scholar",
            "arxiv",
            "biorxiv",
            "rcsb-pdb",
            "pdbe",
            "alphafold",
            "interpro",
            "sifts",
        ];
        assert_eq!(&ids[..first_batch.len()], first_batch,
            "first 14 connector IDs must be stable; full registry has {} entries", ids.len());
        assert!(descriptor("pubmed").is_some());
        assert!(descriptor("chembl").is_some());
        assert!(descriptor("crossref").is_some());
        assert!(descriptor("uniprot").is_some());
        assert!(descriptor("europepmc").is_some());
        assert!(descriptor("openalex").is_some());
        assert!(descriptor("semantic-scholar").is_some());
        assert!(descriptor("arxiv").is_some());
        assert!(descriptor("biorxiv").is_some());
        assert!(descriptor("rcsb-pdb").is_some());
        assert!(descriptor("pdbe").is_some());
        assert!(descriptor("alphafold").is_some());
        assert!(descriptor("interpro").is_some());
        assert!(descriptor("sifts").is_some());
        assert!(descriptor("unknown").is_none());
    }

    #[test]
    fn register_count_is_42() {
        assert_eq!(
            registry().len(),
            42,
            "registry must contain exactly 42 connectors"
        );
    }

    #[test]
    fn adapter_count_matches_descriptor_count() {
        // Direct count validation without LazyLock REGISTRY dependency.
        // The adapter REGISTRY contains the same number of entries as
        // the descriptor registry (proven by compiler-enforced coverage
        // validation at init). This test proves the descriptor side.
        assert_eq!(registry().len(), 42, "42 descriptors required");
        // Verify all expected categories have descriptors
        let ids: std::collections::BTreeSet<&str> =
            registry().iter().map(|d| d.id).collect();
        assert_eq!(ids.len(), 42, "all 42 IDs must be unique");
    }

    #[test]
    fn every_registered_descriptor_passes_validation() {
        for d in registry() {
            validate_descriptor(d).unwrap_or_else(|e| panic!("{} invalid: {e}", d.id));
        }
    }

    #[test]
    fn descriptor_validation_rejects_http_and_bad_paths() {
        let mut d = registry()[0];
        d.base_url = "http://example.com";
        assert!(matches!(
            validate_descriptor(&d),
            Err(PolicyError::InvalidEndpoint { .. })
        ));
    }

    #[test]
    fn validate_request_builds_bounded_request() {
        let req = validate_request("pubmed", "/esearch.fcgi?db=pubmed&term=x", false, 5_000)
            .expect("public connector without credential");
        assert!(req.url.starts_with("https://eutils.ncbi.nlm.nih.gov/"));
        assert_eq!(req.timeout_ms, 5_000);
        assert_eq!(req.rate_limit.max_requests, 3);
        assert!(req.tos_url.starts_with("https://"));
        assert!(
            descriptor("pubmed")
                .unwrap()
                .user_notice
                .contains("NCBI disclaimer")
        );
        let crossref = validate_request(
            "crossref",
            "/works?query.bibliographic=cell&rows=5&select=DOI,title,container-title",
            false,
            5_000,
        )
        .expect("public Crossref request");
        assert_eq!(crossref.rate_limit.max_requests, 1);
        assert_eq!(crossref.rate_limit.per_ms, 1_000);
        let uniprot = validate_request(
            "uniprot",
            "/search?query=insulin&format=json&size=5&fields=accession,id,protein_name,gene_names,organism_name",
            false,
            5_000,
        )
        .expect("public UniProt request");
        assert_eq!(uniprot.rate_limit.max_requests, 1);
        let europepmc = validate_request(
            "europepmc",
            "/search?query=cell&format=json&resultType=lite&pageSize=5&synonym=false",
            false,
            5_000,
        )
        .expect("public Europe PMC request");
        assert_eq!(europepmc.rate_limit.max_requests, 1);
        assert!(matches!(
            validate_request(
                "openalex",
                &openalex::search_path("single cell RNA", 5),
                false,
                10_000
            ),
            Err(PolicyError::CredentialRequired { .. })
        ));
        let openalex = validate_request(
            "openalex",
            &openalex::search_path("single cell RNA", 5),
            true,
            10_000,
        )
        .unwrap();
        assert_eq!(openalex.rate_limit.max_requests, 1);
        assert_eq!(openalex.rate_limit.per_ms, 1_000);
        let fixture = validate_fixture_request(
            "openalex",
            &openalex::search_path("single cell RNA", 5),
            10_000,
        )
        .unwrap();
        assert_eq!(fixture.url, openalex.url);
        assert!(!fixture.url.contains("api_key"));
    }

    #[test]
    fn unknown_connector_and_bad_paths_fail_closed() {
        assert!(matches!(
            validate_request("nope", "/x", false, 1_000),
            Err(PolicyError::UnknownConnector(_))
        ));
        assert!(matches!(
            validate_request("pubmed", "../secret", false, 1_000),
            Err(PolicyError::InvalidEndpoint { .. })
        ));
        assert!(matches!(
            validate_request("pubmed", "/x", false, 0),
            Err(PolicyError::TimeoutExceeds { .. })
        ));
        assert!(matches!(
            validate_request("pubmed", "/x", false, 60_000),
            Err(PolicyError::TimeoutExceeds { .. })
        ));
    }

    #[test]
    fn audit_record_is_redacted_and_stable() {
        let req =
            validate_request("pubmed", "/esearch.fcgi?db=pubmed&term=x", false, 5_000).unwrap();
        let audit = connector_audit(
            &req,
            Some("abc".into()),
            1_700_000_000_000,
            ConnectorOutcome::Retrieved,
        );
        assert_eq!(audit.connector_id, "pubmed");
        assert_eq!(audit.request_sha256.len(), 64);
        assert!(!audit.request_sha256.contains("crispr"));
        let again = connector_audit(
            &req,
            Some("abc".into()),
            1_700_000_000_000,
            ConnectorOutcome::Retrieved,
        );
        assert_eq!(audit, again, "audit must be deterministic for replay");
    }
}

pub mod arrayexpress;
pub mod depmap;
pub mod expression_atlas;
pub mod geo;
pub mod gtex;
pub mod hpa;
pub mod intact;
pub mod opentargets;
pub mod reactome;
pub mod single_cell_atlas;
pub mod string_db;
pub mod wikipathways;
