//! RCSB PDB search API (JSON DSL). Seam contract: S3.
use super::adapter::ProtocolAdapter;
use super::fetch::{ParsedResponse, RetrievedRecord};
use crate::ScienceError;

pub fn search_path(query: &str, max: u32) -> String {
    let rows = max.clamp(1, 50);
    let payload = serde_json::json!({
        "query": {"type":"terminal","service":"full_text","parameters":{"value":query}},
        "return_type":"entry",
        "request_options":{"paginate":{"start":0,"rows":rows}}
    });
    format!("/rcsbsearch/v2/query?json={}", super::url_encode(&payload.to_string()))
}

pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let results = v.get("result_set").and_then(|r| r.as_array())
        .ok_or_else(|| ScienceError::Invalid("rcsb-pdb: missing result_set".into()))?;
    let total = v.get("total_count").and_then(|t| t.as_u64()).unwrap_or(results.len() as u64);
    let mut records = Vec::with_capacity(results.len());
    for r in results {
        let id = r.get("identifier").and_then(|i| i.as_str()).filter(|i| !i.is_empty())
            .ok_or_else(|| ScienceError::Invalid("rcsb-pdb: missing identifier".into()))?;
        let title = r.pointer("/struct/title").and_then(|t| t.as_str()).unwrap_or("(untitled)");
        let container = r.pointer("/rcsb_entry_info/experimental_method").and_then(|m| m.as_str()).unwrap_or("PDB");
        records.push(RetrievedRecord {
            id: id.to_owned(), title: title.to_owned(), container: container.to_owned(),
            url: format!("https://www.rcsb.org/structure/{id}"),
        });
    }
    Ok(ParsedResponse { total_hits: total, records })
}

pub struct RcsbPdbAdapter;
impl ProtocolAdapter for RcsbPdbAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::RCSB_PDB }
    fn expected_exchanges(&self) -> usize { 1 }
    fn build_fixture_paths(&self, q: &str, m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![search_path(q, m)]) }
    fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> {
        if e.len() != 1 { return Err(ScienceError::Invalid("rcsb-pdb: requires 1 exchange".into())); }
        parse_search(&e[0].response)
    }
}

#[cfg(test)] mod tests { use super::*;
    const FIXTURE: &[u8] = br#"{"result_set":[{"identifier":"4HHB","struct":{"title":"Hemoglobin"},"rcsb_entry_info":{"experimental_method":"X-RAY DIFFRACTION"}}],"total_count":1}"#;
    #[test] fn parse_ok() { let p = parse_search(FIXTURE).unwrap(); assert_eq!(p.total_hits,1); assert_eq!(p.records[0].id,"4HHB"); }
    #[test] fn bad() { assert!(parse_search(b"{}").is_err()); assert!(parse_search(br#"{"result_set":[{}]}"#).is_err()); }
}
