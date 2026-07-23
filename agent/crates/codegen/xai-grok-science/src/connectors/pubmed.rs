//! NCBI E-utilities protocol adapter. Seam contract: S3.
//!
//! Pure functions only: relative-path construction and response parsing. No
//! I/O, no sockets, no credentials. The two-step search protocol is
//! `esearch` (hit count + PMID page) followed by `esummary` (titles and
//! journals for those PMIDs), matching NCBI's published usage within the
//! descriptor's 3 requests/second budget.

use super::fetch::RetrievedRecord;
use crate::ScienceError;

/// esearch path for `term`, page size `max`, zero-based page start `offset`.
pub fn esearch_path(term: &str, max: u32, offset: u32) -> String {
    format!(
        "/esearch.fcgi?db=pubmed&retmode=json&retmax={max}&retstart={offset}&term={}",
        super::url_encode(term)
    )
}

/// esummary path for a page of PMIDs from [`parse_esearch`].
pub fn esummary_path(ids: &[String]) -> String {
    format!("/esummary.fcgi?db=pubmed&retmode=json&id={}", ids.join(","))
}

/// Parse an esearch response into (total hits, this page's PMIDs). NCBI
/// reports query errors inside the JSON body rather than the HTTP status, so
/// a well-formed 200 with an errorlist still fails closed here.
pub fn parse_esearch(bytes: &[u8]) -> crate::Result<(u64, Vec<String>)> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let result = value
        .get("esearchresult")
        .ok_or_else(|| ScienceError::Invalid("pubmed esearch: missing esearchresult".into()))?;
    let has_error = result
        .get("errorlist")
        .and_then(|list| list.as_object())
        .is_some_and(|list| !list.is_empty());
    if has_error {
        return Err(ScienceError::Invalid(
            "pubmed esearch: server reported a query error".into(),
        ));
    }
    let total = result
        .get("count")
        .and_then(|count| count.as_str())
        .and_then(|count| count.parse::<u64>().ok())
        .ok_or_else(|| ScienceError::Invalid("pubmed esearch: missing hit count".into()))?;
    let ids = result
        .get("idlist")
        .and_then(|list| list.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|id| id.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    Ok((total, ids))
}

/// Parse an esummary response into retrieved records. `total_hits` is left
/// zero here; the fetch pipeline fills it from the esearch exchange.
pub fn parse_esummary(bytes: &[u8]) -> crate::Result<Vec<RetrievedRecord>> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let result = value
        .get("result")
        .ok_or_else(|| ScienceError::Invalid("pubmed esummary: missing result".into()))?;
    let uids = result
        .get("uids")
        .and_then(|uids| uids.as_array())
        .ok_or_else(|| ScienceError::Invalid("pubmed esummary: missing uids".into()))?;
    let mut records = Vec::with_capacity(uids.len());
    for uid in uids {
        let Some(uid) = uid.as_str() else { continue };
        let item = result.get(uid).ok_or_else(|| {
            ScienceError::Invalid(format!("pubmed esummary: missing record for uid {uid}"))
        })?;
        let title = item
            .get("title")
            .and_then(|title| title.as_str())
            .ok_or_else(|| {
                ScienceError::Invalid(format!("pubmed esummary: uid {uid} has no title"))
            })?;
        let journal = item
            .get("fulljournalname")
            .and_then(|journal| journal.as_str())
            .unwrap_or("(journal not listed)");
        records.push(RetrievedRecord {
            id: uid.to_owned(),
            title: title.to_owned(),
            container: journal.to_owned(),
            url: format!("https://pubmed.ncbi.nlm.nih.gov/{uid}/"),
        });
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ESEARCH: &[u8] = br#"{
        "header": {"type": "esearch", "version": "0.3"},
        "esearchresult": {
            "count": "2", "retmax": "2", "retstart": "0",
            "idlist": ["41234567", "41234568"],
            "translationset": [], "querytranslation": "crispr[All Fields]"
        }
    }"#;

    const ESUMMARY: &[u8] = br#"{
        "header": {"type": "esummary", "version": "0.3"},
        "result": {
            "uids": ["41234567", "41234568"],
            "41234567": {"uid": "41234567", "title": "Base editing advances", "fulljournalname": "Nature"},
            "41234568": {"uid": "41234568", "title": "Prime editing review", "fulljournalname": "Cell"}
        }
    }"#;

    #[test]
    fn esearch_path_encodes_terms_and_paginates() {
        let path = esearch_path("crispr base editing & cancer", 5, 10);
        assert!(path.starts_with("/esearch.fcgi?db=pubmed&retmode=json&retmax=5&retstart=10&term="));
        assert!(path.contains("crispr%20base%20editing%20%26%20cancer"));
        assert!(!path.contains(".."));
    }

    #[test]
    fn parse_esearch_reads_count_and_ids() {
        let (total, ids) = parse_esearch(ESEARCH).unwrap();
        assert_eq!(total, 2);
        assert_eq!(ids, vec!["41234567".to_owned(), "41234568".to_owned()]);
    }

    #[test]
    fn parse_esearch_fails_closed_on_server_error_and_malformed() {
        let err = br#"{"esearchresult": {"count": "0", "errorlist": {"phraseignored": ["xyz"]}}}"#;
        assert!(parse_esearch(err).is_err());
        assert!(parse_esearch(b"not json").is_err());
        assert!(parse_esearch(br#"{"esearchresult": {}}"#).is_err());
    }

    #[test]
    fn parse_esummary_reads_records_in_uid_order() {
        let records = parse_esummary(ESUMMARY).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].id, "41234567");
        assert_eq!(records[0].title, "Base editing advances");
        assert_eq!(records[0].container, "Nature");
        assert_eq!(records[0].url, "https://pubmed.ncbi.nlm.nih.gov/41234567/");
    }

    #[test]
    fn parse_esummary_fails_closed_on_missing_record() {
        let broken = br#"{"result": {"uids": ["1"], "2": {"title": "orphan"}}}"#;
        assert!(parse_esummary(broken).is_err());
    }

    /// L5 live probe: real NCBI retrieval. Explicitly ignored; run with:
    /// `cargo test -p xai-grok-science live_probe_pubmed -- --ignored --nocapture`
    /// and archive the printed PUBMED_LIVE_EVIDENCE line.
    #[tokio::test]
    #[ignore = "live network probe against NCBI; run explicitly"]
    async fn live_probe_pubmed_real_search() {
        let query = "crispr base editing";
        let client = reqwest::Client::new();
        let search = super::super::validate_request(
            "pubmed",
            &esearch_path(query, 3, 0),
            false,
            10_000,
        )
        .unwrap();
        let body = client
            .get(&search.url)
            .send()
            .await
            .expect("esearch send")
            .error_for_status()
            .expect("esearch status")
            .bytes()
            .await
            .expect("esearch body");
        let (total, ids) = parse_esearch(&body).expect("parse live esearch");
        assert!(total > 0, "live esearch returned no hits");
        assert!(!ids.is_empty(), "live esearch returned no ids");
        // Respect the descriptor's 3 req/s budget between the two calls.
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let summary = super::super::validate_request(
            "pubmed",
            &esummary_path(&ids),
            false,
            10_000,
        )
        .unwrap();
        let body = client
            .get(&summary.url)
            .send()
            .await
            .expect("esummary send")
            .error_for_status()
            .expect("esummary status")
            .bytes()
            .await
            .expect("esummary body");
        let records = parse_esummary(&body).expect("parse live esummary");
        assert_eq!(records.len(), ids.len());
        let first = &records[0];
        let evidence = serde_json::json!({
            "connector": "pubmed",
            "query": query,
            "total_hits": total,
            "first_record": {
                "pmid": first.id,
                "title": first.title,
                "journal": first.container,
                "url": first.url,
            },
            "retrieved_at": chrono::Utc::now().to_rfc3339(),
            "request_urls": [search.url, summary.url],
            "tos_url": super::super::descriptor("pubmed").unwrap().tos_url,
        });
        println!(
            "PUBMED_LIVE_EVIDENCE={}",
            serde_json::to_string_pretty(&evidence).unwrap()
        );
    }
}
