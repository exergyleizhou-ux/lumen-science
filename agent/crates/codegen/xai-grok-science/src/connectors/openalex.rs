//! OpenAlex Works REST protocol adapter. Seam contract: S3.
//!
//! The v1 operation performs one bounded works search and uses `select` to
//! return only identifiers, title, and publication year. Abstracts, full text,
//! authorships, locations, references, topics, and content URLs are neither
//! returned nor parsed.

use super::adapter::ProtocolAdapter;
use super::fetch::{ParsedResponse, RetrievedRecord};
use crate::ScienceError;

/// DS-1 protocol adapter. Registered in the global [`super::adapter::REGISTRY`].
pub struct OpenalexAdapter;

impl ProtocolAdapter for OpenalexAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor {
        &super::OPENALEX
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
                "openalex fetch requires exactly one works exchange".into(),
            ));
        }
        parse_search(&exchanges[0].response)
    }
}

const SELECTED_FIELDS: &str = "id,doi,display_name,publication_year";

/// Build one bounded OpenAlex works search. The product boundary enforces
/// `1..=50`; clamping here keeps direct callers bounded too.
pub fn search_path(term: &str, max: u32) -> String {
    let per_page = max.clamp(1, 50);
    format!(
        "/works?search={}&per_page={per_page}&select={SELECTED_FIELDS}",
        super::url_encode(term)
    )
}

/// Parse only the selected bibliographic fields. A partial malformed result
/// fails the whole run closed before any response artifact is registered.
pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let total_hits = value
        .pointer("/meta/count")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| ScienceError::Invalid("openalex search: missing meta.count".into()))?;
    let results = value
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| ScienceError::Invalid("openalex search: missing results".into()))?;
    let mut records = Vec::with_capacity(results.len());
    for result in results {
        let id = result
            .get("id")
            .and_then(serde_json::Value::as_str)
            .filter(|value| valid_work_id(value))
            .ok_or_else(|| {
                ScienceError::Invalid("openalex search: result without valid work id".into())
            })?;
        let title = result
            .get("display_name")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                ScienceError::Invalid("openalex search: result without display_name".into())
            })?;
        let year = result
            .get("publication_year")
            .and_then(serde_json::Value::as_u64)
            .filter(|year| (1..=9999).contains(year))
            .ok_or_else(|| {
                ScienceError::Invalid(
                    "openalex search: result without valid publication_year".into(),
                )
            })?;
        let doi = result
            .get("doi")
            .and_then(serde_json::Value::as_str)
            .filter(|value| valid_doi_url(value));
        let container = doi.map_or_else(|| year.to_string(), |doi| format!("{year} · {doi}"));
        records.push(RetrievedRecord {
            id: id.trim_start_matches("https://openalex.org/").to_owned(),
            title: title.to_owned(),
            container,
            url: id.to_owned(),
        });
    }
    Ok(ParsedResponse {
        total_hits,
        records,
    })
}

fn valid_work_id(value: &str) -> bool {
    value
        .strip_prefix("https://openalex.org/W")
        .is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn valid_doi_url(value: &str) -> bool {
    value
        .strip_prefix("https://doi.org/")
        .is_some_and(|suffix| {
            !suffix.is_empty()
                && !suffix
                    .bytes()
                    .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &[u8] = br#"{
      "meta": {
        "count": 12,
        "db_response_time_ms": 7,
        "page": 1,
        "per_page": 2,
        "cost_usd": 0.001
      },
      "results": [
        {
          "id": "https://openalex.org/W1234567890",
          "doi": "https://doi.org/10.5555/example.1",
          "display_name": "Reproducible scholarly graphs",
          "publication_year": 2026,
          "abstract_inverted_index": {"INTENTIONALLY_IGNORED": [0]},
          "authorships": [{"raw_author_name": "INTENTIONALLY_IGNORED"}],
          "content_url": "https://api.openalex.org/works/W123/content"
        },
        {
          "id": "https://openalex.org/W987654321",
          "doi": null,
          "display_name": "Durable evidence for open science",
          "publication_year": 2025,
          "referenced_works": ["INTENTIONALLY_IGNORED"]
        }
      ],
      "group_by": []
    }"#;

    #[test]
    fn search_path_is_bounded_and_selects_only_bibliographic_fields() {
        let path = search_path("single cell RNA", 500);
        assert_eq!(
            path,
            "/works?search=single%20cell%20RNA&per_page=50&select=id,doi,display_name,publication_year"
        );
        assert!(!path.contains("abstract"));
        assert!(!path.contains("fulltext"));
        assert!(!path.contains("authorship"));
        assert!(!path.contains("content"));
    }

    #[test]
    fn parse_search_reads_only_selected_bibliographic_fields() {
        let parsed = parse_search(SEARCH).unwrap();
        assert_eq!(parsed.total_hits, 12);
        assert_eq!(parsed.records.len(), 2);
        assert_eq!(parsed.records[0].id, "W1234567890");
        assert_eq!(parsed.records[0].title, "Reproducible scholarly graphs");
        assert_eq!(
            parsed.records[0].container,
            "2026 · https://doi.org/10.5555/example.1"
        );
        assert_eq!(parsed.records[0].url, "https://openalex.org/W1234567890");
        assert_eq!(parsed.records[1].container, "2025");
    }

    #[test]
    fn parse_search_fails_closed_on_malformed_records() {
        assert!(parse_search(b"not json").is_err());
        assert!(parse_search(br#"{"results":[]}"#).is_err());
        assert!(parse_search(br#"{"meta":{"count":1}}"#).is_err());
        assert!(
            parse_search(
                br#"{"meta":{"count":1},"results":[{"id":"http://openalex.org/W1","display_name":"x","publication_year":2026}]}"#
            )
            .is_err()
        );
        assert!(
            parse_search(
                br#"{"meta":{"count":1},"results":[{"id":"https://openalex.org/W1","display_name":" ","publication_year":2026}]}"#
            )
            .is_err()
        );
        assert!(
            parse_search(
                br#"{"meta":{"count":1},"results":[{"id":"https://openalex.org/W1","display_name":"x","publication_year":0}]}"#
            )
            .is_err()
        );
    }

    #[tokio::test]
    #[ignore = "live metered network probe against OpenAlex; run explicitly"]
    async fn live_probe_openalex_real_search() {
        let api_key = std::env::var("OPENALEX_API_KEY")
            .expect("live OpenAlex probe requires runtime-only OPENALEX_API_KEY");
        assert!(
            !api_key.trim().is_empty(),
            "OPENALEX_API_KEY must not be empty"
        );
        let query = "single cell RNA";
        let request =
            super::super::validate_request("openalex", &search_path(query, 1), true, 10_000)
                .expect("validated OpenAlex request");
        let body = reqwest::Client::new()
            .get(&request.url)
            .query(&[("api_key", api_key.as_str())])
            .send()
            .await
            .expect("OpenAlex send")
            .error_for_status()
            .expect("OpenAlex status")
            .bytes()
            .await
            .expect("OpenAlex body");
        let parsed = parse_search(&body).expect("parse live OpenAlex search");
        let first = parsed
            .records
            .first()
            .expect("live OpenAlex returned no records");
        let evidence = serde_json::json!({
            "connector": "openalex",
            "query": query,
            "total_hits": parsed.total_hits,
            "first_record": {
                "openalex_id": first.id,
                "title": first.title,
                "publication": first.container,
                "url": first.url,
            },
            "retrieved_at": chrono::Utc::now().to_rfc3339(),
            "tos_url": super::super::descriptor("openalex").unwrap().tos_url,
        });
        println!(
            "OPENALEX_LIVE_EVIDENCE={}",
            serde_json::to_string_pretty(&evidence).unwrap()
        );
    }
}
