//! Crossref Works REST protocol adapter. Seam contract: S3.
//!
//! Pure functions build one bounded list query and parse only DOI, title, and
//! container-title bibliographic fields. Abstract and full-text fields are
//! intentionally neither selected nor consumed because their rights differ
//! from Crossref's factual metadata.

use super::adapter::ProtocolAdapter;
use super::fetch::{ParsedResponse, RetrievedRecord};
use crate::ScienceError;

/// DS-1 protocol adapter. Registered in the global [`super::adapter::REGISTRY`].
pub struct CrossrefAdapter;

impl ProtocolAdapter for CrossrefAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor {
        &super::CROSSREF
    }

    fn expected_exchanges(&self) -> usize {
        1
    }

    fn build_fixture_paths(
        &self,
        query: &str,
        max_results: u32,
        _fixtures: &[Vec<u8>],
    ) -> crate::Result<Vec<String>> {
        Ok(vec![works_path(query, max_results)])
    }

    fn parse_responses(
        &self,
        exchanges: &[super::fetch::FetchExchange],
    ) -> crate::Result<ParsedResponse> {
        if exchanges.len() != 1 {
            return Err(ScienceError::Invalid(
                "crossref fetch requires exactly one works exchange".into(),
            ));
        }
        parse_works(&exchanges[0].response)
    }
}

const SELECTED_FIELDS: &str = "DOI,title,container-title";

/// Build a bounded bibliographic works search. The product boundary already
/// enforces `1..=50`; this helper clamps defensively for direct callers.
pub fn works_path(term: &str, max: u32) -> String {
    let rows = max.clamp(1, 50);
    format!(
        "/works?query.bibliographic={}&rows={rows}&select={SELECTED_FIELDS}",
        super::url_encode(term)
    )
}

/// Append the operator-provided Crossref contact identity for an explicitly
/// authorized live probe. The email remains in the ephemeral request only.
pub fn works_path_with_mailto(term: &str, max: u32, mailto: &str) -> crate::Result<String> {
    let mailto = mailto.trim();
    if mailto.is_empty()
        || mailto.bytes().any(|byte| byte.is_ascii_whitespace())
        || !mailto.contains('@')
    {
        return Err(ScienceError::Invalid(
            "crossref live probe requires a valid non-empty CROSSREF_MAILTO".into(),
        ));
    }
    Ok(format!(
        "{}&mailto={}",
        works_path(term, max),
        super::url_encode(mailto)
    ))
}

/// Parse a Crossref `/works` list response. Every returned item must have a
/// DOI and non-empty title; malformed partial records fail the run closed.
pub fn parse_works(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    if value.get("status").and_then(|status| status.as_str()) != Some("ok") {
        return Err(ScienceError::Invalid(
            "crossref works: response status is not ok".into(),
        ));
    }
    let message = value
        .get("message")
        .ok_or_else(|| ScienceError::Invalid("crossref works: missing message".into()))?;
    let total_hits = message
        .get("total-results")
        .and_then(|total| total.as_u64())
        .ok_or_else(|| ScienceError::Invalid("crossref works: missing total-results".into()))?;
    let items = message
        .get("items")
        .and_then(|items| items.as_array())
        .ok_or_else(|| ScienceError::Invalid("crossref works: missing items".into()))?;
    let mut records = Vec::with_capacity(items.len());
    for item in items {
        let doi = item
            .get("DOI")
            .and_then(|doi| doi.as_str())
            .filter(|doi| {
                !doi.is_empty()
                    && !doi
                        .bytes()
                        .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
            })
            .ok_or_else(|| {
                ScienceError::Invalid("crossref works: item without valid DOI".into())
            })?;
        let title = first_non_empty_string(item.get("title"))
            .ok_or_else(|| ScienceError::Invalid("crossref works: item without title".into()))?;
        let container =
            first_non_empty_string(item.get("container-title")).unwrap_or("(unknown container)");
        records.push(RetrievedRecord {
            id: doi.to_owned(),
            title: title.to_owned(),
            container: container.to_owned(),
            url: format!("https://doi.org/{}", super::url_encode(doi)),
        });
    }
    Ok(ParsedResponse {
        total_hits,
        records,
    })
}

fn first_non_empty_string(value: Option<&serde_json::Value>) -> Option<&str> {
    value?
        .as_array()?
        .iter()
        .filter_map(serde_json::Value::as_str)
        .find(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORKS: &[u8] = br#"{
        "status": "ok",
        "message-type": "work-list",
        "message": {
            "total-results": 12,
            "items": [
                {
                    "DOI": "10.5555/example.1",
                    "title": ["Reproducible science workflows"],
                    "container-title": ["Journal of Durable Research"],
                    "abstract": "not consumed"
                },
                {
                    "DOI": "10.5555/example.2",
                    "title": ["Metadata without an abstract"],
                    "container-title": []
                }
            ]
        }
    }"#;

    #[test]
    fn works_path_encodes_query_clamps_rows_and_selects_safe_fields() {
        let path = works_path("single cell RNA", 500);
        assert_eq!(
            path,
            "/works?query.bibliographic=single%20cell%20RNA&rows=50&select=DOI,title,container-title"
        );
        assert!(!path.contains("abstract"));
        assert!(!path.contains("link"));
    }

    #[test]
    fn mailto_path_requires_and_encodes_contact_identity() {
        assert!(works_path_with_mailto("cell", 1, "").is_err());
        assert!(works_path_with_mailto("cell", 1, "not-an-email").is_err());
        let path = works_path_with_mailto("cell", 1, "science@example.org").unwrap();
        assert!(path.ends_with("&mailto=science%40example.org"));
    }

    #[test]
    fn parse_works_reads_only_bibliographic_fields() {
        let parsed = parse_works(WORKS).unwrap();
        assert_eq!(parsed.total_hits, 12);
        assert_eq!(parsed.records.len(), 2);
        assert_eq!(parsed.records[0].id, "10.5555/example.1");
        assert_eq!(parsed.records[0].title, "Reproducible science workflows");
        assert_eq!(parsed.records[0].container, "Journal of Durable Research");
        assert_eq!(parsed.records[0].url, "https://doi.org/10.5555%2Fexample.1");
        assert_eq!(parsed.records[1].container, "(unknown container)");
    }

    #[test]
    fn parse_works_fails_closed_on_malformed_records() {
        assert!(parse_works(b"not json").is_err());
        assert!(parse_works(br#"{"status":"error","message":{}}"#).is_err());
        assert!(
            parse_works(
                br#"{"status":"ok","message":{"total-results":1,"items":[{"title":["x"]}]}}"#
            )
            .is_err()
        );
        assert!(
            parse_works(br#"{"status":"ok","message":{"total-results":1,"items":[{"DOI":"10/x","title":[]}]}}"#)
                .is_err()
        );
    }

    /// L5 live probe: an explicitly authorized Crossref request. It requires
    /// a runtime-only contact identity and never prints that identity or URL.
    #[tokio::test]
    #[ignore = "live network probe against Crossref; requires explicit authorization and CROSSREF_MAILTO"]
    async fn live_probe_crossref_real_search() {
        use sha2::{Digest, Sha256};

        let mailto = std::env::var("CROSSREF_MAILTO")
            .expect("set CROSSREF_MAILTO only for an explicitly authorized live probe");
        let query = "single cell RNA sequencing";
        let path = works_path_with_mailto(query, 1, &mailto).expect("validated contact identity");
        let request = super::super::validate_request("crossref", &path, false, 10_000)
            .expect("validated Crossref request");
        let body = reqwest::Client::new()
            .get(&request.url)
            .send()
            .await
            .expect("Crossref send")
            .error_for_status()
            .expect("Crossref status")
            .bytes()
            .await
            .expect("Crossref body");
        let parsed = parse_works(&body).expect("parse live Crossref works");
        let first = parsed
            .records
            .first()
            .expect("live Crossref returned no records");
        let evidence = serde_json::json!({
            "connector": "crossref",
            "query": query,
            "total_hits": parsed.total_hits,
            "first_record": {
                "doi": first.id,
                "title": first.title,
                "container": first.container,
                "url": first.url,
            },
            "retrieved_at": chrono::Utc::now().to_rfc3339(),
            "request_sha256": format!("{:x}", Sha256::digest(request.url.as_bytes())),
            "tos_url": super::super::descriptor("crossref").unwrap().tos_url,
        });
        println!(
            "CROSSREF_LIVE_EVIDENCE={}",
            serde_json::to_string_pretty(&evidence).unwrap()
        );
    }
}
