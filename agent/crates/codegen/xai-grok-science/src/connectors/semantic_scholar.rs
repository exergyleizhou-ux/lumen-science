//! Semantic Scholar Academic Graph API protocol adapter. Seam contract: S3.
//!
//! Pure functions build one bounded search and parse only bibliographic fields
//! (paperId, title, year, venue, externalIds, url). Abstracts, citation counts,
//! authors, references, and citations are neither requested nor parsed.

use super::adapter::ProtocolAdapter;
use super::fetch::{ParsedResponse, RetrievedRecord};
use crate::ScienceError;

/// Build a bounded Semantic Scholar paper search. Only safe bibliographic
/// fields are selected; abstract and author data are excluded.
pub fn search_path(term: &str, max: u32) -> String {
    let limit = max.clamp(1, 50);
    format!(
        "/graph/v1/paper/search?query={}&limit={limit}&fields=paperId,title,url,year,venue,externalIds",
        super::url_encode(term)
    )
}

/// Parse a Semantic Scholar search response. Every result must have a
/// non-empty paperId and title; partial malformed records fail closed.
pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let total_hits = value
        .get("total")
        .and_then(|t| t.as_u64())
        .ok_or_else(|| ScienceError::Invalid("semantic-scholar search: missing total".into()))?;
    let data = value
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| ScienceError::Invalid("semantic-scholar search: missing data".into()))?;
    let mut records = Vec::with_capacity(data.len());
    for paper in data {
        let paper_id = paper
            .get("paperId")
            .and_then(|id| id.as_str())
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                ScienceError::Invalid("semantic-scholar search: paper without paperId".into())
            })?;
        let title = paper
            .get("title")
            .and_then(|t| t.as_str())
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| {
                ScienceError::Invalid("semantic-scholar search: paper without title".into())
            })?;
        let container = paper
            .get("venue")
            .and_then(|v| v.as_str())
            .filter(|v| !v.trim().is_empty())
            .unwrap_or("(unknown venue)");
        records.push(RetrievedRecord {
            id: paper_id.to_owned(),
            title: title.to_owned(),
            container: container.to_owned(),
            url: paper
                .get("url")
                .and_then(|u| u.as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    format!("https://www.semanticscholar.org/paper/{paper_id}")
                }),
        });
    }
    Ok(ParsedResponse { total_hits, records })
}

/// DS-1 protocol adapter. Registered in the global [`super::adapter::REGISTRY`].
pub struct SemanticScholarAdapter;

impl ProtocolAdapter for SemanticScholarAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor {
        &super::SEMANTIC_SCHOLAR
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
        Ok(vec![search_path(query, max_results)])
    }

    fn parse_responses(
        &self,
        exchanges: &[super::fetch::FetchExchange],
    ) -> crate::Result<ParsedResponse> {
        if exchanges.len() != 1 {
            return Err(ScienceError::Invalid(
                "semantic-scholar fetch requires exactly one search exchange".into(),
            ));
        }
        parse_search(&exchanges[0].response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &[u8] = br#"{
        "total": 42,
        "data": [
            {
                "paperId": "abc123def456",
                "title": "Attention Is All You Need",
                "url": "https://www.semanticscholar.org/paper/abc123",
                "year": 2017,
                "venue": "NeurIPS",
                "externalIds": {"DOI": "10.5555/example.1", "ArXiv": "1706.03762"},
                "citationCount": 99999,
                "abstract": "intentionally not consumed",
                "authors": [{"name": "not consumed"}]
            },
            {
                "paperId": "xyz789",
                "title": "BERT: Pre-training of Deep Bidirectional Transformers",
                "venue": "",
                "url": null,
                "externalIds": {}
            }
        ]
    }"#;

    #[test]
    fn search_path_selects_safe_fields_and_clamps_limit() {
        let path = search_path("machine learning", 500);
        assert!(path.starts_with("/graph/v1/paper/search?query=machine%20learning&limit=50"));
        assert!(path.contains("paperId,title,url,year,venue,externalIds"));
        assert!(!path.contains("abstract"));
        assert!(!path.contains("citationCount"));
        assert!(!path.contains("authors"));
    }

    #[test]
    fn parse_search_reads_bibliographic_fields_and_ignores_excluded() {
        let parsed = parse_search(SEARCH).unwrap();
        assert_eq!(parsed.total_hits, 42);
        assert_eq!(parsed.records.len(), 2);
        assert_eq!(parsed.records[0].id, "abc123def456");
        assert_eq!(parsed.records[0].title, "Attention Is All You Need");
        assert_eq!(parsed.records[0].container, "NeurIPS");
        assert!(parsed.records[0].url.contains("semanticscholar.org"));
        // Empty venue → placeholder.
        assert_eq!(parsed.records[1].container, "(unknown venue)");
        // Null url → constructed fallback.
        assert!(parsed.records[1].url.contains("semanticscholar.org/paper/xyz789"));
    }

    #[test]
    fn parse_search_fails_closed_on_malformed_records() {
        assert!(parse_search(b"not json").is_err());
        assert!(parse_search(br#"{"total":1}"#).is_err());
        assert!(parse_search(br#"{"total":1,"data":[{"title":"x"}]}"#).is_err());
        assert!(parse_search(br#"{"total":1,"data":[{"paperId":"x","title":""}]}"#).is_err());
        assert!(
            parse_search(br#"{"total":1,"data":[{"paperId":"x","title":"y"}],"_excluded_sentinel":"abstract"}"#)
                .is_ok()
        );
    }

    /// L5 live probe: real Semantic Scholar retrieval. Explicitly ignored;
    /// run with `cargo test -p xai-grok-science live_probe_semantic_scholar
    /// -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "live network probe against Semantic Scholar; run explicitly"]
    async fn live_probe_semantic_scholar_real_search() {
        let query = "machine learning";
        let request = super::super::validate_request(
            "semantic-scholar",
            &search_path(query, 3),
            false,
            10_000,
        )
        .expect("validated Semantic Scholar request");
        let body = reqwest::Client::new()
            .get(&request.url)
            .send()
            .await
            .expect("Semantic Scholar send")
            .error_for_status()
            .expect("Semantic Scholar status")
            .bytes()
            .await
            .expect("Semantic Scholar body");
        let parsed = parse_search(&body).expect("parse live Semantic Scholar search");
        assert!(parsed.total_hits > 0, "live Semantic Scholar returned no hits");
        let first = parsed.records.first().expect("no records");
        let evidence = serde_json::json!({
            "connector": "semantic-scholar",
            "query": query,
            "total_hits": parsed.total_hits,
            "first_record": {
                "paper_id": first.id,
                "title": first.title,
                "url": first.url,
            },
            "retrieved_at": chrono::Utc::now().to_rfc3339(),
            "request_url": request.url,
            "tos_url": super::super::descriptor("semantic-scholar").unwrap().tos_url,
        });
        println!(
            "SEMANTIC_SCHOLAR_LIVE_EVIDENCE={}",
            serde_json::to_string_pretty(&evidence).unwrap()
        );
    }
}
