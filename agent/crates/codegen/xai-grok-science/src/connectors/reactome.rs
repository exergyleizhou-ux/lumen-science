//! Reactome ContentService. Seam: S3.
use super::adapter::ProtocolAdapter; use super::fetch::{ParsedResponse, RetrievedRecord}; use crate::ScienceError;
pub fn search_path(query: &str) -> String { format!("/ContentService/search/query?query={}&cluster=true&species=Homo+sapiens", super::url_encode(query)) }
pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let results = v.get("results").and_then(|r| r.as_array()).ok_or_else(|| ScienceError::Invalid("reactome: missing results".into()))?;
    let total = v.get("totalResults").and_then(|t| t.as_u64()).unwrap_or(results.len() as u64);
    let mut records = Vec::with_capacity(results.len());
    for r in results {
        let id1 = r.get("stId").and_then(|i| i.as_str()).map(|s| s.to_owned());
        let id2 = r.get("dbId").and_then(|d| d.as_u64()).map(|n| format!("R-HSA-{n}"));
        let id = id1.or(id2).filter(|i| !i.is_empty()).ok_or_else(|| ScienceError::Invalid("reactome: missing id".into()))?;
        let name = r.get("name").and_then(|n| n.as_str()).unwrap_or(&id).to_owned();
        records.push(RetrievedRecord { id: id.to_string(), title: name, container: "Reactome".to_owned(), url: format!("https://reactome.org/content/detail/{id}") });
    }
    Ok(ParsedResponse { total_hits: total, records })
}
pub struct ReactomeAdapter;
impl ProtocolAdapter for ReactomeAdapter { fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::REACTOME } fn expected_exchanges(&self) -> usize { 1 } fn build_fixture_paths(&self, q: &str, _m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![search_path(q)]) } fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> { if e.len() != 1 { return Err(ScienceError::Invalid("reactome: 1 exchange".into())); } parse_search(&e[0].response) } }
#[cfg(test)] mod tests { use super::*; const F: &[u8] = br#"{"totalResults":1,"results":[{"stId":"R-HSA-164843","name":"DNA damage response","dbId":164843}]}"#; #[test] fn ok() { let p = parse_search(F).unwrap(); assert_eq!(p.total_hits,1); } }
