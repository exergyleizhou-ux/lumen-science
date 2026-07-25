//! SureChEMBL API. Seam: S3. POST search.
use super::adapter::ProtocolAdapter;
use super::fetch::{ParsedResponse, RetrievedRecord};
use crate::ScienceError;

pub fn search_query(query: &str, max: u32) -> String { format!("query={}&page=1&itemsPerPage={}", super::url_encode(query), max.clamp(1,50)) }

pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let docs = v.pointer("/data/results/documents").and_then(|d| d.as_array()).or_else(|| v.get("documents").and_then(|d| d.as_array()))
        .ok_or_else(|| ScienceError::Invalid("surechembl: missing documents".into()))?;
    let mut records = Vec::with_capacity(docs.len());
    for doc in docs {
        let id = doc.get("docId").and_then(|d| d.as_u64()).map(|n| n.to_string()).or_else(|| doc.pointer("/metadata/pn").and_then(|p| p.as_str()).map(|s| s.to_owned())).unwrap_or_default();
        if id.is_empty() { continue; }
        let title = doc.pointer("/metadata/titles").and_then(|t| t.as_array()).and_then(|a| a.first()).and_then(|t| t.as_str()).unwrap_or(&id).to_owned();
        records.push(RetrievedRecord {
            id: id.clone(), title, container: "SureChEMBL".to_owned(),
            url: format!("https://www.surechembl.org/patent/{id}"),
        });
    }
    Ok(ParsedResponse { total_hits: records.len() as u64, records })
}

pub struct SurechemblAdapter;
impl ProtocolAdapter for SurechemblAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::SURECHEMBL }
    fn expected_exchanges(&self) -> usize { 1 }
    fn build_fixture_paths(&self, q: &str, m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![format!("/api/search/content?{}", search_query(q, m))]) }
    fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> {
        if e.len() != 1 { return Err(ScienceError::Invalid("surechembl: requires 1 exchange".into())); }
        parse_search(&e[0].response)
    }
}

#[cfg(test)] mod tests { use super::*;
    const F: &[u8] = br#"{"data":{"results":{"documents":[{"docId":12345,"metadata":{"titles":["Aspirin synthesis"],"pn":"US1234567"}}]}}}"#;
    #[test] fn parse_ok() { let p = parse_search(F).unwrap(); assert_eq!(p.total_hits,1); assert_eq!(p.records[0].id,"12345"); }
    #[test] fn bad() { assert!(parse_search(b"{}").is_err()); }
}
