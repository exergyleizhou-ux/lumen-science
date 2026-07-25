//! Ensembl REST API. Seam: S3.
use super::adapter::ProtocolAdapter; use super::fetch::{ParsedResponse, RetrievedRecord}; use crate::ScienceError;
pub fn lookup_path(symbol: &str, species: &str) -> String { format!("/lookup/symbol/{}/{}?content-type=application/json", species, symbol) }
pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let id = v.get("id").and_then(|i| i.as_str()).filter(|i| !i.is_empty()).ok_or_else(|| ScienceError::Invalid("ensembl: missing id".into()))?;
    let name = v.get("display_name").and_then(|n| n.as_str()).unwrap_or(id);
    let desc = v.get("description").and_then(|d| d.as_str()).unwrap_or("");
    Ok(ParsedResponse { total_hits: 1, records: vec![RetrievedRecord { id: id.to_owned(), title: format!("{name} {desc}").trim().to_owned(), container: v.get("biotype").and_then(|b| b.as_str()).unwrap_or("Ensembl").to_owned(), url: format!("https://www.ensembl.org/id/{id}") }] })
}
pub struct EnsemblAdapter;
impl ProtocolAdapter for EnsemblAdapter { fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::ENSEMBL } fn expected_exchanges(&self) -> usize { 1 } fn build_fixture_paths(&self, q: &str, _m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![lookup_path(q, "human")]) } fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> { if e.len() != 1 { return Err(ScienceError::Invalid("ensembl: 1 exchange".into())); } parse_search(&e[0].response) } }
#[cfg(test)] mod tests { use super::*; const F: &[u8] = br#"{"id":"ENSG00000139618","display_name":"BRCA2","biotype":"protein_coding","description":"BRCA2 DNA repair associated"}"#;
    #[test] fn ok() { let p = parse_search(F).unwrap(); assert_eq!(p.records[0].id,"ENSG00000139618"); }
}
