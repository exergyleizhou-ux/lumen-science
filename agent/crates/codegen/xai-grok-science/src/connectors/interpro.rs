//! InterPro API. Seam contract: S3.
use super::adapter::ProtocolAdapter;
use super::fetch::{ParsedResponse, RetrievedRecord};
use crate::ScienceError;

pub fn search_path(query: &str, max: u32) -> String {
    let size = max.clamp(1, 50);
    format!("/interpro/api/entry/interpro/?search={}&page_size={size}", super::url_encode(query))
}

pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let results = v.get("results").and_then(|r| r.as_array())
        .ok_or_else(|| ScienceError::Invalid("interpro: missing results".into()))?;
    let total = v.get("count").and_then(|c| c.as_u64()).unwrap_or(results.len() as u64);
    let mut records = Vec::with_capacity(results.len());
    for r in results {
        let meta = r.get("metadata").ok_or_else(|| ScienceError::Invalid("interpro: missing metadata".into()))?;
        let id = meta.get("accession").and_then(|a| a.as_str()).filter(|a| !a.is_empty())
            .ok_or_else(|| ScienceError::Invalid("interpro: missing accession".into()))?;
        let name = meta.get("name");
        let title = if let Some(s) = name.and_then(|n| n.as_str()) { s.to_owned() }
            else { name.and_then(|n| n.get("name")).and_then(|n| n.as_str()).unwrap_or(id).to_owned() };
        let container = meta.get("source_database").and_then(|s| s.as_str()).unwrap_or("InterPro");
        records.push(RetrievedRecord {
            id: id.to_owned(), title, container: container.to_owned(),
            url: format!("https://www.ebi.ac.uk/interpro/entry/InterPro/{id}/"),
        });
    }
    Ok(ParsedResponse { total_hits: total, records })
}

pub struct InterproAdapter;
impl ProtocolAdapter for InterproAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::INTERPRO }
    fn expected_exchanges(&self) -> usize { 1 }
    fn build_fixture_paths(&self, q: &str, m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![search_path(q, m)]) }
    fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> {
        if e.len() != 1 { return Err(ScienceError::Invalid("interpro: requires 1 exchange".into())); }
        parse_search(&e[0].response)
    }
}

#[cfg(test)] mod tests { use super::*;
    const F: &[u8] = br#"{"count":1,"results":[{"metadata":{"accession":"IPR000001","name":"Kringle","source_database":"pfam"}}]}"#;
    #[test] fn parse_ok() { let p = parse_search(F).unwrap(); assert_eq!(p.total_hits,1); assert_eq!(p.records[0].id,"IPR000001"); }
    #[test] fn bad() { assert!(parse_search(b"{}").is_err()); }
}
