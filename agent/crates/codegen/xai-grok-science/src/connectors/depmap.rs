//! DepMap Portal API. Seam: S3.
use super::adapter::ProtocolAdapter; use super::fetch::{ParsedResponse, RetrievedRecord}; use crate::ScienceError;
pub fn catalog_path() -> String { "/portal/api/download/files".to_string() }
pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes).or_else(|_| Ok::<_, crate::ScienceError>(serde_json::Value::Null)).unwrap_or_default();
    if v.is_null() { return Err(ScienceError::Invalid("depmap: non-JSON response (likely anti-bot HTML)".into())); }
    let arr = v.as_array().unwrap_or(&super::EMPTY_JSON_ARRAY);
    let mut recs = Vec::with_capacity(arr.len());
    for f in arr { let name = f.get("fileName").and_then(|n| n.as_str()).unwrap_or(""); if name.is_empty() { continue; } let desc = f.get("fileDescription").and_then(|d| d.as_str()).unwrap_or(name); recs.push(RetrievedRecord { id: name.to_owned(), title: desc.to_owned(), container: "DepMap".to_owned(), url: f.get("downloadUrl").and_then(|u| u.as_str()).unwrap_or("https://depmap.org").to_owned() }); }
    Ok(ParsedResponse { total_hits: recs.len() as u64, records: recs })
}
pub struct DepmapAdapter;
impl ProtocolAdapter for DepmapAdapter { fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::DEPMAP } fn expected_exchanges(&self) -> usize { 1 } fn build_fixture_paths(&self, _q: &str, _m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![catalog_path()]) } fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> { if e.len() != 1 { return Err(ScienceError::Invalid("depmap: 1 exchange".into())); } parse_search(&e[0].response) } }
#[cfg(test)] mod tests { use super::*; const F: &[u8] = br#"[{"releaseName":"24Q2","fileName":"CRISPR_gene_effect.csv","fileDescription":"Gene effect scores","downloadUrl":"https://depmap.org/download"}]"#; #[test] fn ok() { let p = parse_search(F).unwrap(); assert_eq!(p.total_hits,1); } }
