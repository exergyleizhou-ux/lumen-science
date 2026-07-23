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

pub mod chembl;
pub mod fetch;
pub mod pubmed;

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

/// All registered connectors, in stable order.
pub fn registry() -> &'static [ConnectorDescriptor] {
    &[PUBMED, CHEMBL]
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
    if d.id.is_empty() || !d.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
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
        assert_eq!(ids, vec!["pubmed", "chembl"]);
        assert!(descriptor("pubmed").is_some());
        assert!(descriptor("chembl").is_some());
        assert!(descriptor("unknown").is_none());
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
