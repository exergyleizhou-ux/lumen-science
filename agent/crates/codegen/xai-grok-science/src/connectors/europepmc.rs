//! Europe PMC Articles REST protocol adapter. Seam contract: S3.
//!
//! The v1 operation performs one bounded `lite` metadata search with synonym
//! expansion disabled for deterministic query semantics. Abstracts, full text,
//! reference lists, annotations, and links are neither requested nor parsed.

use super::adapter::ProtocolAdapter;
use super::fetch::{ParsedResponse, RetrievedRecord};
use crate::ScienceError;

/// DS-1 protocol adapter. Registered in the global [`super::adapter::REGISTRY`].
pub struct EuropepmcAdapter;

impl ProtocolAdapter for EuropepmcAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor {
        &super::EUROPEPMC
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
                "europepmc fetch requires exactly one search exchange".into(),
            ));
        }
        parse_search(&exchanges[0].response)
    }
}

/// Build one bounded Europe PMC metadata search. The product boundary enforces
/// `1..=50`; clamping here keeps direct callers bounded too.
pub fn search_path(term: &str, max: u32) -> String {
    let page_size = max.clamp(1, 50);
    format!(
        "/search?query={}&format=json&resultType=lite&pageSize={page_size}&synonym=false",
        super::url_encode(term)
    )
}

/// Parse only the stable identifiers and bibliographic summary fields exposed
/// by the `lite` response. A partial malformed result fails the whole run
/// closed before any response artifact is registered.
pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let hit_count = value
        .get("hitCount")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| ScienceError::Invalid("europepmc search: missing hitCount".into()))?;
    let results = value
        .pointer("/resultList/result")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            ScienceError::Invalid("europepmc search: missing resultList.result".into())
        })?;
    let mut records = Vec::with_capacity(results.len());
    for result in results {
        let source = result
            .get("source")
            .and_then(serde_json::Value::as_str)
            .filter(|value| valid_identifier(value))
            .ok_or_else(|| {
                ScienceError::Invalid("europepmc search: result without valid source".into())
            })?;
        let id = result
            .get("id")
            .and_then(serde_json::Value::as_str)
            .filter(|value| valid_identifier(value))
            .ok_or_else(|| {
                ScienceError::Invalid("europepmc search: result without valid id".into())
            })?;
        let title = result
            .get("title")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                ScienceError::Invalid("europepmc search: result without title".into())
            })?;
        let journal = result
            .get("journalTitle")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let year = result
            .get("pubYear")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let container = match (journal, year) {
            (Some(journal), Some(year)) => format!("{journal} ({year})"),
            (Some(journal), None) => journal.to_owned(),
            (None, Some(year)) => year.to_owned(),
            (None, None) => "(unknown publication)".to_owned(),
        };
        records.push(RetrievedRecord {
            id: format!("{source}:{id}"),
            title: title.to_owned(),
            container,
            url: format!("https://europepmc.org/article/{source}/{id}"),
        });
    }
    Ok(ParsedResponse {
        total_hits: hit_count,
        records,
    })
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.'
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &[u8] = br#"{
      "version": "6.9",
      "hitCount": 12,
      "nextCursorMark": "AoIIP4",
      "resultList": {
        "result": [
          {
            "id": "41234567",
            "source": "MED",
            "pmid": "41234567",
            "doi": "10.5555/example.1",
            "title": "Reproducible single-cell analysis",
            "authorString": "Doe J, Roe R",
            "journalTitle": "Genome Methods",
            "pubYear": "2026",
            "abstractText": "INTENTIONALLY_IGNORED"
          },
          {
            "id": "PPR123456",
            "source": "PPR",
            "title": "A durable literature workflow",
            "pubYear": "2025"
          }
        ]
      }
    }"#;

    #[test]
    fn search_path_is_bounded_lite_json_and_disables_synonyms() {
        let path = search_path("single cell RNA", 500);
        assert_eq!(
            path,
            "/search?query=single%20cell%20RNA&format=json&resultType=lite&pageSize=50&synonym=false"
        );
        assert!(!path.contains("core"));
        assert!(!path.contains("fullText"));
        assert!(!path.contains("abstract"));
    }

    #[test]
    fn parse_search_reads_only_lite_bibliographic_fields() {
        let parsed = parse_search(SEARCH).unwrap();
        assert_eq!(parsed.total_hits, 12);
        assert_eq!(parsed.records.len(), 2);
        assert_eq!(parsed.records[0].id, "MED:41234567");
        assert_eq!(parsed.records[0].title, "Reproducible single-cell analysis");
        assert_eq!(parsed.records[0].container, "Genome Methods (2026)");
        assert_eq!(
            parsed.records[0].url,
            "https://europepmc.org/article/MED/41234567"
        );
        assert_eq!(parsed.records[1].container, "2025");
    }

    #[test]
    fn parse_search_fails_closed_on_malformed_records() {
        assert!(parse_search(b"not json").is_err());
        assert!(parse_search(br#"{"hitCount":1}"#).is_err());
        assert!(parse_search(br#"{"resultList":{"result":[]}}"#).is_err());
        assert!(
            parse_search(
                br#"{"hitCount":1,"resultList":{"result":[{"source":"MED","title":"x"}]}}"#
            )
            .is_err()
        );
        assert!(
            parse_search(
                br#"{"hitCount":1,"resultList":{"result":[{"source":"M/ED","id":"1","title":"x"}]}}"#
            )
            .is_err()
        );
        assert!(
            parse_search(
                br#"{"hitCount":1,"resultList":{"result":[{"source":"MED","id":"1","title":"  "}]}}"#
            )
            .is_err()
        );
    }

    #[tokio::test]
    #[ignore = "live network probe against Europe PMC; run explicitly"]
    async fn live_probe_europepmc_real_search() {
        let query = "single cell RNA";
        let request =
            super::super::validate_request("europepmc", &search_path(query, 1), false, 10_000)
                .expect("validated Europe PMC request");
        let body = reqwest::Client::new()
            .get(&request.url)
            .send()
            .await
            .expect("Europe PMC send")
            .error_for_status()
            .expect("Europe PMC status")
            .bytes()
            .await
            .expect("Europe PMC body");
        let parsed = parse_search(&body).expect("parse live Europe PMC search");
        let first = parsed
            .records
            .first()
            .expect("live Europe PMC returned no records");
        let evidence = serde_json::json!({
            "connector": "europepmc",
            "query": query,
            "total_hits": parsed.total_hits,
            "first_record": {
                "source_and_id": first.id,
                "title": first.title,
                "publication": first.container,
                "url": first.url,
            },
            "retrieved_at": chrono::Utc::now().to_rfc3339(),
            "tos_url": super::super::descriptor("europepmc").unwrap().tos_url,
        });
        println!(
            "EUROPEPMC_LIVE_EVIDENCE={}",
            serde_json::to_string_pretty(&evidence).unwrap()
        );
    }
}
