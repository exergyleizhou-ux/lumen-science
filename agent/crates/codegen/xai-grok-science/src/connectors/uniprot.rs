//! UniProtKB website REST protocol adapter. Seam contract: S3.
//!
//! Pure functions build one bounded search request and parse only identity,
//! protein-name, gene-name, and organism summary fields. Protein sequences,
//! features, references, and citation text are neither requested nor parsed.

use super::adapter::ProtocolAdapter;
use super::fetch::{ParsedResponse, RetrievedRecord};
use crate::ScienceError;

/// DS-1 protocol adapter. Registered in the global [`super::adapter::REGISTRY`].
pub struct UniprotAdapter;

impl ProtocolAdapter for UniprotAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor {
        &super::UNIPROT
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
                "uniprot fetch requires exactly one search exchange".into(),
            ));
        }
        parse_search(&exchanges[0].response)
    }
}

const FIELDS: &str = "accession,id,protein_name,gene_names,organism_name";

/// Build a bounded UniProtKB search. The product boundary enforces `1..=50`;
/// clamping here keeps direct callers bounded as well.
pub fn search_path(term: &str, max: u32) -> String {
    let size = max.clamp(1, 50);
    format!(
        "/search?query={}&format=json&size={size}&fields={FIELDS}",
        super::url_encode(term)
    )
}

/// Parse the bounded UniProtKB JSON result. Every record requires a primary
/// accession and a stable display name; partial malformed records fail closed.
pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let results = value
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| ScienceError::Invalid("uniprot search: missing results".into()))?;
    let total_hits = value
        .get("totalResults")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| ScienceError::Invalid("uniprot search: missing totalResults".into()))?;
    let mut records = Vec::with_capacity(results.len());
    for result in results {
        let accession = result
            .get("primaryAccession")
            .and_then(serde_json::Value::as_str)
            .filter(|value| valid_identifier(value))
            .ok_or_else(|| {
                ScienceError::Invalid("uniprot search: result without valid accession".into())
            })?;
        let entry_id = result
            .get("uniProtkbId")
            .and_then(serde_json::Value::as_str)
            .filter(|value| valid_identifier(value));
        let protein_name = result
            .pointer("/proteinDescription/recommendedName/fullName/value")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                result
                    .pointer("/proteinDescription/submissionNames/0/fullName/value")
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| !value.trim().is_empty())
            })
            .or(entry_id)
            .ok_or_else(|| {
                ScienceError::Invalid("uniprot search: result without protein name or id".into())
            })?;
        let organism = result
            .pointer("/organism/scientificName")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("(unknown organism)");
        records.push(RetrievedRecord {
            id: accession.to_owned(),
            title: protein_name.to_owned(),
            container: organism.to_owned(),
            url: format!("https://www.uniprot.org/uniprotkb/{accession}/entry"),
        });
    }
    Ok(ParsedResponse {
        total_hits,
        records,
    })
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &[u8] = br#"{
      "results": [
        {
          "primaryAccession": "P01308",
          "uniProtkbId": "INS_HUMAN",
          "proteinDescription": {
            "recommendedName": {"fullName": {"value": "Insulin"}}
          },
          "genes": [{"geneName": {"value": "INS"}}],
          "organism": {"scientificName": "Homo sapiens"},
          "sequence": {"value": "INTENTIONALLY_IGNORED"}
        },
        {
          "primaryAccession": "A0A000",
          "uniProtkbId": "A0A000_TEST",
          "proteinDescription": {
            "submissionNames": [{"fullName": {"value": "Submitted protein"}}]
          },
          "organism": {}
        }
      ],
      "totalResults": 42
    }"#;

    #[test]
    fn search_path_encodes_query_clamps_size_and_selects_summary_fields() {
        let path = search_path("human insulin", 500);
        assert_eq!(
            path,
            "/search?query=human%20insulin&format=json&size=50&fields=accession,id,protein_name,gene_names,organism_name"
        );
        assert!(!path.contains("sequence"));
        assert!(!path.contains("feature"));
        assert!(!path.contains("citation"));
    }

    #[test]
    fn parse_search_reads_summary_and_ignores_sequence() {
        let parsed = parse_search(SEARCH).unwrap();
        assert_eq!(parsed.total_hits, 42);
        assert_eq!(parsed.records.len(), 2);
        assert_eq!(parsed.records[0].id, "P01308");
        assert_eq!(parsed.records[0].title, "Insulin");
        assert_eq!(parsed.records[0].container, "Homo sapiens");
        assert_eq!(
            parsed.records[0].url,
            "https://www.uniprot.org/uniprotkb/P01308/entry"
        );
        assert_eq!(parsed.records[1].container, "(unknown organism)");
    }

    #[test]
    fn parse_search_fails_closed_on_malformed_records() {
        assert!(parse_search(b"not json").is_err());
        assert!(parse_search(br#"{"totalResults":1}"#).is_err());
        assert!(parse_search(br#"{"results":[]}"#).is_err());
        assert!(parse_search(br#"{"results":[{"uniProtkbId":"X"}],"totalResults":1}"#).is_err());
        assert!(
            parse_search(
                br#"{"results":[{"primaryAccession":"P 1","uniProtkbId":"X"}],"totalResults":1}"#
            )
            .is_err()
        );
    }

    #[tokio::test]
    #[ignore = "live network probe against UniProt; run explicitly"]
    async fn live_probe_uniprot_real_search() {
        let query = "insulin";
        let request =
            super::super::validate_request("uniprot", &search_path(query, 1), false, 10_000)
                .expect("validated UniProt request");
        let body = reqwest::Client::new()
            .get(&request.url)
            .send()
            .await
            .expect("UniProt send")
            .error_for_status()
            .expect("UniProt status")
            .bytes()
            .await
            .expect("UniProt body");
        let parsed = parse_search(&body).expect("parse live UniProt search");
        let first = parsed
            .records
            .first()
            .expect("live UniProt returned no records");
        let evidence = serde_json::json!({
            "connector": "uniprot",
            "query": query,
            "total_hits": parsed.total_hits,
            "first_record": {
                "accession": first.id,
                "protein_name": first.title,
                "organism": first.container,
                "url": first.url,
            },
            "retrieved_at": chrono::Utc::now().to_rfc3339(),
            "tos_url": super::super::descriptor("uniprot").unwrap().tos_url,
        });
        println!(
            "UNIPROT_LIVE_EVIDENCE={}",
            serde_json::to_string_pretty(&evidence).unwrap()
        );
    }
}
