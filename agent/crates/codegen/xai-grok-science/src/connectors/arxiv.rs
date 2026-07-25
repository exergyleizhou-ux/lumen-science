//! arXiv Atom query API protocol adapter. Seam contract: S3.
//!
//! arXiv returns Atom XML, not JSON. Parsing uses minimal string matching
//! to avoid dragging in an XML crate dependency. The v1 operation performs
//! one bounded search query and parses only identifier and metadata fields;
//! abstracts (summary) and PDF links are intentionally excluded.
//!
//! Rate limit: arXiv asks for ≤1 request per 3 seconds.

use super::adapter::ProtocolAdapter;
use super::fetch::{ParsedResponse, RetrievedRecord};
use crate::ScienceError;

/// Build a bounded arXiv Atom search query. Wraps bare queries in `all:`.
pub fn search_path(term: &str, max: u32) -> String {
    let limit = max.clamp(1, 50);
    let expr = if term.chars().any(|c| c == ':') {
        term.to_string()
    } else {
        format!("all:{term}")
    };
    // base_url is https://export.arxiv.org/api — path must be /query (not /api/query).
    format!(
        "/query?search_query={}&start=0&max_results={limit}&sortBy=relevance",
        super::url_encode(&expr)
    )
}

/// Extract the text content of an XML element (first occurrence).
fn text_between(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)?;
    let text = xml[start..start + end].trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

/// Extract all blocks between <tag> and </tag>. Simple, not recursive.
fn blocks_between(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut results = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let content_start = start + open.len();
        if let Some(end) = rest[content_start..].find(&close) {
            results.push(rest[content_start..content_start + end].to_string());
            rest = &rest[content_start + end + close.len()..];
        } else {
            break;
        }
    }
    results
}

/// Parse an arXiv Atom feed response into a ParsedResponse. Only bibliographic
/// fields are extracted; abstracts and PDFs are intentionally excluded.
pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let xml = std::str::from_utf8(bytes)
        .map_err(|_| ScienceError::Invalid("arxiv: response is not valid UTF-8".into()))?;

    if !xml.contains("<feed") {
        return Err(ScienceError::Invalid(
            "arxiv: response is not an Atom feed (possibly rate-limited or unavailable)".into(),
        ));
    }

    let entries = blocks_between(xml, "entry");
    if entries.is_empty() && xml.contains("<entry>") {
        return Err(ScienceError::Invalid("arxiv: no entries found in feed".into()));
    }

    let total_hits = entries.len() as u64;
    let mut records = Vec::with_capacity(entries.len());

    for entry in &entries {
        // arXiv error entries point at .../api/errors
        if let Some(id) = text_between(entry, "id")
            && id.contains("arxiv.org/api/errors")
        {
            return Err(ScienceError::Invalid(format!(
                "arxiv: server rejected query: {id}"
            )));
        }

        let arxiv_id = text_between(entry, "id")
            .map(|id| id.replace("http://arxiv.org/abs/", "").replace("https://arxiv.org/abs/", ""))
            .unwrap_or_default();

        if arxiv_id.is_empty() {
            return Err(ScienceError::Invalid("arxiv: entry without valid id".into()));
        }

        let title = text_between(entry, "title")
            .ok_or_else(|| ScienceError::Invalid("arxiv: entry without title".into()))?;

        let container = text_between(entry, "arxiv:primary_category")
            .or_else(|| {
                // Self-closing tag: <arxiv:primary_category term="cs.CL"/>
                if let Some(start) = entry.find("<arxiv:primary_category") {
                    let rest = &entry[start..];
                    if let Some(term_start) = rest.find("term=\"") {
                        let after = &rest[term_start + 6..];
                        if let Some(term_end) = after.find('\"') {
                            return Some(after[..term_end].to_string());
                        }
                    }
                }
                entry.find("arxiv:primary_category").map(|_| "arXiv".to_string())
            })
            .unwrap_or_else(|| "arXiv".to_string());

        records.push(RetrievedRecord {
            id: arxiv_id.clone(),
            title,
            container,
            url: format!("https://arxiv.org/abs/{arxiv_id}"),
        });
    }

    Ok(ParsedResponse { total_hits, records })
}

/// DS-1 protocol adapter. Registered in the global [`super::adapter::REGISTRY`].
pub struct ArxivAdapter;

impl ProtocolAdapter for ArxivAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor {
        &super::ARXIV
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
                "arxiv fetch requires exactly one search exchange".into(),
            ));
        }
        parse_search(&exchanges[0].response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>http://arxiv.org/abs/1706.03762</id>
    <title>Attention Is All You Need</title>
    <summary>intentionally not consumed</summary>
    <published>2017-06-12T00:00:00Z</published>
    <author><name>Vaswani</name></author>
    <arxiv:primary_category term="cs.CL"/>
    <link title="pdf" href="http://arxiv.org/pdf/1706.03762"/>
  </entry>
  <entry>
    <id>http://arxiv.org/abs/1810.04805</id>
    <title>BERT: Pre-training of Deep Bidirectional Transformers</title>
    <published>2018-10-11T00:00:00Z</published>
    <author><name>Devlin</name></author>
  </entry>
</feed>"#;

    #[test]
    fn search_path_wraps_bare_query_in_all() {
        let path = search_path("transformer attention", 50);
        assert!(path.contains("search_query=all%3Atransformer%20attention"));
        assert!(!path.contains("abstract"));
    }

    #[test]
    fn search_path_passes_fielded_query_through() {
        let path = search_path("ti:transformer AND cat:cs.LG", 10);
        assert!(path.contains("search_query=ti%3Atransformer%20AND%20cat%3Acs.LG"));
    }

    #[test]
    fn parse_search_reads_entries_and_ignores_summary_and_pdf() {
        let parsed = parse_search(FEED.as_bytes()).unwrap();
        assert_eq!(parsed.total_hits, 2);
        assert_eq!(parsed.records.len(), 2);
        assert_eq!(parsed.records[0].id, "1706.03762");
        assert_eq!(parsed.records[0].title, "Attention Is All You Need");
        assert_eq!(parsed.records[0].container, "cs.CL");
        assert_eq!(parsed.records[0].url, "https://arxiv.org/abs/1706.03762");
        assert_eq!(parsed.records[1].id, "1810.04805");
        assert_eq!(parsed.records[1].title, "BERT: Pre-training of Deep Bidirectional Transformers");
        assert_eq!(parsed.records[1].container, "arXiv");
    }

    #[test]
    fn parse_search_fails_closed_on_non_atom() {
        assert!(parse_search(b"<html>error</html>").is_err());
        assert!(parse_search(b"not xml").is_err());
    }

    #[test]
    fn parse_search_rejects_error_entry() {
        let err_feed = r#"<feed><entry><id>http://arxiv.org/api/errors</id><title>Error</title></entry></feed>"#;
        assert!(parse_search(err_feed.as_bytes()).is_err());
    }

    /// L5 live probe: real arXiv retrieval. Explicitly ignored.
    #[tokio::test]
    #[ignore = "live network probe against arXiv; run explicitly"]
    async fn live_probe_arxiv_real_search() {
        let query = "machine learning";
        let request = super::super::validate_request(
            "arxiv",
            &search_path(query, 3),
            false,
            10_000,
        )
        .expect("validated arXiv request");
        let body = reqwest::Client::new()
            .get(&request.url)
            .send()
            .await
            .expect("arXiv send")
            .error_for_status()
            .expect("arXiv status")
            .bytes()
            .await
            .expect("arXiv body");
        let parsed = parse_search(&body).expect("parse live arXiv");
        assert!(parsed.total_hits > 0, "live arXiv returned no hits");
        let first = parsed.records.first().expect("no records");
        let evidence = serde_json::json!({
            "connector": "arxiv",
            "query": query,
            "total_hits": parsed.total_hits,
            "first_record": { "id": first.id, "title": first.title },
            "retrieved_at": chrono::Utc::now().to_rfc3339(),
            "request_url": request.url,
            "tos_url": super::super::descriptor("arxiv").unwrap().tos_url,
        });
        println!("ARXIV_LIVE_EVIDENCE={}", serde_json::to_string_pretty(&evidence).unwrap());
    }
}
