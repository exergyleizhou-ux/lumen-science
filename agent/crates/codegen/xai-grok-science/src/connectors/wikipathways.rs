//! WikiPathways JSON API. Seam: S3. Full catalog client-side filter.
use super::adapter::ProtocolAdapter; use super::fetch::{ParsedResponse, RetrievedRecord}; use crate::ScienceError;
pub fn catalog_path() -> String { "/json/findPathwaysByText.json".to_string() }
pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let info = v.get("pathwayInfo").and_then(|p| p.as_array()).ok_or_else(|| ScienceError::Invalid("wikipathways: missing pathwayInfo".into()))?;
    let mut records = Vec::with_capacity(info.len());
    for p in info {
        let id = p.get("id").and_then(|i| i.as_str()).filter(|i| !i.is_empty()).ok_or_else(|| ScienceError::Invalid("wikipathways: missing id".into()))?;
        let name = p.get("name").and_then(|n| n.as_str()).unwrap_or(id);
        let species = p.get("species").and_then(|s| s.as_str()).unwrap_or("");
        records.push(RetrievedRecord { id: id.to_owned(), title: name.to_owned(), container: species.to_owned(), url: p.get("url").and_then(|u| u.as_str()).unwrap_or(&format!("https://www.wikipathways.org/index.php/Pathway:{id}")).to_owned() });
    }
    Ok(ParsedResponse { total_hits: records.len() as u64, records })
}
pub struct WikpathwaysAdapter;
impl ProtocolAdapter for WikpathwaysAdapter { fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::WIKIPATHWAYS } fn expected_exchanges(&self) -> usize { 1 } fn build_fixture_paths(&self, _q: &str, _m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![catalog_path()]) } fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> { if e.len() != 1 { return Err(ScienceError::Invalid("wikipathways: 1 exchange".into())); } parse_search(&e[0].response) } }
#[cfg(test)] mod tests { use super::*; const F: &[u8] = br#"{"pathwayInfo":[{"id":"WP100","name":"DNA damage response","species":"Homo sapiens","url":"https://www.wikipathways.org/index.php/Pathway:WP100"}]}"#; #[test] fn ok() { let p = parse_search(F).unwrap(); assert_eq!(p.total_hits,1); } }
