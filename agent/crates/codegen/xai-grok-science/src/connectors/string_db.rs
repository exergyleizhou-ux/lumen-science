//! STRING DB API. Seam: S3.
use super::adapter::ProtocolAdapter; use super::fetch::{ParsedResponse, RetrievedRecord}; use crate::ScienceError;
pub fn search_path(query: &str, max: u32) -> String { format!("/api/json/get_string_ids?identifiers={}&species=9606&limit={max}&echo_query=1", super::url_encode(query)) }
pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let arr = v.as_array().ok_or_else(|| ScienceError::Invalid("string-db: not an array".into()))?;
    let mut records = Vec::with_capacity(arr.len());
    for item in arr {
        let id = item.get("stringId").and_then(|s| s.as_str()).or_else(|| item.get("preferredName").and_then(|s| s.as_str())).filter(|s| !s.is_empty()).ok_or_else(|| ScienceError::Invalid("string-db: missing id".into()))?;
        let name = item.get("preferredName").and_then(|n| n.as_str()).unwrap_or(id);
        let org = item.get("taxonName").and_then(|t| t.as_str()).unwrap_or("");
        records.push(RetrievedRecord { id: id.to_owned(), title: name.to_owned(), container: org.to_owned(), url: format!("https://string-db.org/network/9606.{id}") });
    }
    Ok(ParsedResponse { total_hits: records.len() as u64, records })
}
pub struct StringDbAdapter;
impl ProtocolAdapter for StringDbAdapter { fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::STRING_DB } fn expected_exchanges(&self) -> usize { 1 } fn build_fixture_paths(&self, q: &str, m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![search_path(q, m)]) } fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> { if e.len() != 1 { return Err(ScienceError::Invalid("string-db: 1 exchange".into())); } parse_search(&e[0].response) } }
#[cfg(test)] mod tests { use super::*; const F: &[u8] = br#"[{"stringId":"9606.ENSP00000380156","preferredName":"BRCA2","ncbiTaxonId":9606,"taxonName":"Homo sapiens"}]"#; #[test] fn ok() { let p = parse_search(F).unwrap(); assert_eq!(p.total_hits,1); } }
