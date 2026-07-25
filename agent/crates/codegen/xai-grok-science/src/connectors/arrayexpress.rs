//! ArrayExpress / BioStudies. Seam: S3.
use super::adapter::ProtocolAdapter; use super::fetch::{ParsedResponse, RetrievedRecord}; use crate::ScienceError;
pub fn search_path(query: &str, max: u32) -> String { format!("/biostudies/api/v1/arrayexpress/search?query={}&pageSize={}", super::url_encode(query), max.clamp(1,50)) }
pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let study_list: Vec<serde_json::Value> = v.get("_embedded").and_then(|e| e.get("studies")).and_then(|s| s.as_array()).cloned()
        .or_else(|| v.as_array().cloned()).unwrap_or_default();
    let mut recs = Vec::with_capacity(study_list.len());
    for r in &study_list { let acc = r.get("accession").and_then(|a| a.as_str()).unwrap_or(""); if acc.is_empty() { continue; } let title = r.get("title").and_then(|t| t.as_str()).unwrap_or(acc); recs.push(RetrievedRecord { id: acc.to_owned(), title: title.to_owned(), container: "ArrayExpress".to_owned(), url: format!("https://www.ebi.ac.uk/biostudies/arrayexpress/studies/{acc}") }); }
    Ok(ParsedResponse { total_hits: recs.len() as u64, records: recs })
}
pub struct ArrayexpressAdapter;
impl ProtocolAdapter for ArrayexpressAdapter { fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::ARRAYEXPRESS } fn expected_exchanges(&self) -> usize { 1 } fn build_fixture_paths(&self, q: &str, m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![search_path(q, m)]) } fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> { if e.len() != 1 { return Err(ScienceError::Invalid("arrayexpress: 1 exchange".into())); } parse_search(&e[0].response) } }
#[cfg(test)] mod tests { use super::*; const F: &[u8] = br#"[{"accession":"E-MTAB-1234","title":"RNA-seq of cancer cells"}]"#; #[test] fn ok() { let p = parse_search(F).unwrap(); assert_eq!(p.total_hits,1); } }
