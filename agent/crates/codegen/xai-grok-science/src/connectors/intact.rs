//! IntAct WS API. Seam: S3.
use super::adapter::ProtocolAdapter; use super::fetch::{ParsedResponse, RetrievedRecord}; use crate::ScienceError;
pub fn search_path(query: &str, max: u32) -> String { format!("/intact/ws/interaction/findInteractions/{}?page=0&pageSize={}", super::url_encode(query), max.clamp(1,50)) }
pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let content = v.get("content").and_then(|c| c.as_array()).ok_or_else(|| ScienceError::Invalid("intact: missing content".into()))?;
    let total = v.get("totalElements").and_then(|t| t.as_u64()).unwrap_or(content.len() as u64);
    let mut records = Vec::with_capacity(content.len());
    for c in content {
        let ac = c.get("ac").and_then(|a| a.as_str()).unwrap_or("");
        if ac.is_empty() { continue; }
        let a_name = c.get("moleculeA").and_then(|m| m.as_str()).unwrap_or("");
        let b_name = c.get("moleculeB").and_then(|m| m.as_str()).unwrap_or("");
        let title = format!("{a_name} — {b_name}").trim().to_owned();
        records.push(RetrievedRecord { id: ac.to_owned(), title, container: "IntAct".to_owned(), url: format!("https://www.ebi.ac.uk/intact/interaction/{ac}") });
    }
    Ok(ParsedResponse { total_hits: total, records })
}
pub struct IntactAdapter;
impl ProtocolAdapter for IntactAdapter { fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::INTACT } fn expected_exchanges(&self) -> usize { 1 } fn build_fixture_paths(&self, q: &str, m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![search_path(q, m)]) } fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> { if e.len() != 1 { return Err(ScienceError::Invalid("intact: 1 exchange".into())); } parse_search(&e[0].response) } }
#[cfg(test)] mod tests { use super::*; const F: &[u8] = br#"{"totalElements":1,"content":[{"ac":"EBI-12345","moleculeA":"BRCA2","moleculeB":"RAD51"}]}"#; #[test] fn ok() { let p = parse_search(F).unwrap(); assert_eq!(p.total_hits,1); } }
