//! MyGene.info v3 API. Seam: S3.
use super::adapter::ProtocolAdapter; use super::fetch::{ParsedResponse, RetrievedRecord}; use crate::ScienceError;
pub fn search_path(term: &str, max: u32) -> String { format!("/v3/query?q={}&size={max}&species=human", super::url_encode(term)) }
pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let hits = v.get("hits").and_then(|h| h.as_array()).ok_or_else(|| ScienceError::Invalid("mygene: missing hits".into()))?;
    let total = v.get("total").and_then(|t| t.as_u64()).unwrap_or(hits.len() as u64);
    let mut records = Vec::with_capacity(hits.len());
    for h in hits {
        let id = h.get("_id").and_then(|i| i.as_str()).ok_or_else(|| ScienceError::Invalid("mygene: missing _id".into()))?;
        let name = h.get("symbol").and_then(|s| s.as_str()).unwrap_or(id);
        let desc = h.get("name").and_then(|n| n.as_str()).unwrap_or("");
        records.push(RetrievedRecord { id: id.to_owned(), title: format!("{name} {desc}").trim().to_owned(), container: "MyGene.info".to_owned(), url: format!("https://mygene.info/v3/gene/{id}") });
    }
    Ok(ParsedResponse { total_hits: total, records })
}
pub struct MygeneAdapter;
impl ProtocolAdapter for MygeneAdapter { fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::MYGENE } fn expected_exchanges(&self) -> usize { 1 } fn build_fixture_paths(&self, q: &str, m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![search_path(q, m)]) } fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> { if e.len() != 1 { return Err(ScienceError::Invalid("mygene: 1 exchange".into())); } parse_search(&e[0].response) } }
#[cfg(test)] mod tests { use super::*; const F: &[u8] = br#"{"total":1,"hits":[{"_id":"672","symbol":"BRCA1","name":"BRCA1 DNA repair associated"}]}"#; #[test] fn ok() { let p = parse_search(F).unwrap(); assert_eq!(p.total_hits,1); } }
