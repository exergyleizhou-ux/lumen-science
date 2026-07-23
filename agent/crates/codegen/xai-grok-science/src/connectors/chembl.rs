//! EBI ChEMBL REST protocol adapter. Seam contract: S3.
//!
//! Pure functions only: relative-path construction and response parsing. The
//! v1 operation is a full-text molecule search returning one page of records.

use super::fetch::{ParsedResponse, RetrievedRecord};
use crate::ScienceError;

/// Molecule full-text search path for `term`, page size `max`, zero-based
/// page `offset`.
pub fn search_path(term: &str, max: u32, offset: u32) -> String {
    format!(
        "/molecule/search.json?q={}&limit={max}&offset={offset}",
        super::url_encode(term)
    )
}

/// Parse a molecule search response. `pref_name` may be null for uncurated
/// compounds; those records are kept with an explicit placeholder because a
/// missing name is data, not an error. A missing `molecule_chembl_id` is
/// malformed and fails closed.
pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let molecules = value
        .get("molecules")
        .and_then(|molecules| molecules.as_array())
        .ok_or_else(|| ScienceError::Invalid("chembl search: missing molecules".into()))?;
    let total_hits = value
        .pointer("/page_meta/total_count")
        .and_then(|count| count.as_u64())
        .ok_or_else(|| ScienceError::Invalid("chembl search: missing total_count".into()))?;
    let mut records = Vec::with_capacity(molecules.len());
    for molecule in molecules {
        let id = molecule
            .get("molecule_chembl_id")
            .and_then(|id| id.as_str())
            .ok_or_else(|| {
                ScienceError::Invalid("chembl search: molecule without chembl id".into())
            })?;
        let title = molecule
            .get("pref_name")
            .and_then(|name| name.as_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("(unnamed compound)");
        records.push(RetrievedRecord {
            id: id.to_owned(),
            title: title.to_owned(),
            container: "ChEMBL".to_owned(),
            url: format!("https://www.ebi.ac.uk/chembl/compound_report_card/{id}/"),
        });
    }
    Ok(ParsedResponse { total_hits, records })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &[u8] = br#"{
        "molecules": [
            {"molecule_chembl_id": "CHEMBL25", "pref_name": "ASPIRIN"},
            {"molecule_chembl_id": "CHEMBL3137343", "pref_name": null}
        ],
        "page_meta": {"limit": 5, "offset": 0, "total_count": 42}
    }"#;

    #[test]
    fn search_path_encodes_terms_and_paginates() {
        let path = search_path("acetylsalicylic acid", 5, 20);
        assert_eq!(
            path,
            "/molecule/search.json?q=acetylsalicylic%20acid&limit=5&offset=20"
        );
    }

    #[test]
    fn parse_search_reads_records_and_total() {
        let parsed = parse_search(SEARCH).unwrap();
        assert_eq!(parsed.total_hits, 42);
        assert_eq!(parsed.records.len(), 2);
        assert_eq!(parsed.records[0].id, "CHEMBL25");
        assert_eq!(parsed.records[0].title, "ASPIRIN");
        assert_eq!(
            parsed.records[0].url,
            "https://www.ebi.ac.uk/chembl/compound_report_card/CHEMBL25/"
        );
        // Null pref_name is data, not an error.
        assert_eq!(parsed.records[1].title, "(unnamed compound)");
    }

    #[test]
    fn parse_search_fails_closed_on_malformed() {
        assert!(parse_search(b"not json").is_err());
        assert!(parse_search(br#"{"page_meta": {"total_count": 1}}"#).is_err());
        assert!(parse_search(br#"{"molecules": [], "page_meta": {}}"#).is_err());
        assert!(
            parse_search(
                br#"{"molecules": [{"pref_name": "X"}], "page_meta": {"total_count": 1}}"#
            )
            .is_err()
        );
    }

    /// L5 live probe: real public ChEMBL retrieval. Explicitly ignored; run
    /// with `cargo test -p xai-grok-science live_probe_chembl -- --ignored
    /// --nocapture` and archive the printed evidence line.
    #[tokio::test]
    #[ignore = "live network probe against ChEMBL; run explicitly"]
    async fn live_probe_chembl_real_search() {
        let query = "aspirin";
        let request = super::super::validate_request(
            "chembl",
            &search_path(query, 3, 0),
            false,
            10_000,
        )
        .expect("validated ChEMBL request");
        let body = reqwest::Client::new()
            .get(&request.url)
            .send()
            .await
            .expect("ChEMBL send")
            .error_for_status()
            .expect("ChEMBL status")
            .bytes()
            .await
            .expect("ChEMBL body");
        let parsed = parse_search(&body).expect("parse live ChEMBL search");
        assert!(parsed.total_hits > 0, "live ChEMBL returned no hits");
        let first = parsed.records.first().expect("live ChEMBL returned no records");
        let evidence = serde_json::json!({
            "connector": "chembl",
            "query": query,
            "total_hits": parsed.total_hits,
            "first_record": {
                "chembl_id": first.id,
                "name": first.title,
                "url": first.url,
            },
            "retrieved_at": chrono::Utc::now().to_rfc3339(),
            "request_url": request.url,
            "tos_url": super::super::descriptor("chembl").unwrap().tos_url,
        });
        println!(
            "CHEMBL_LIVE_EVIDENCE={}",
            serde_json::to_string_pretty(&evidence).unwrap()
        );
    }
}
