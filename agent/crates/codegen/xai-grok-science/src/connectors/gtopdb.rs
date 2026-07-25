//! GtoPdb (Guide to PHARMACOLOGY) REST API. Seam: S3.
use super::adapter::ProtocolAdapter;
use super::fetch::{ParsedResponse, RetrievedRecord};
use crate::ScienceError;

pub fn search_path(query: &str) -> String { format!("/services/ligands?name={}", super::url_encode(query)) }

pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let arr = v.as_array().ok_or_else(|| ScienceError::Invalid("gtopdb: not an array".into()))?;
    let mut records = Vec::with_capacity(arr.len());
    for ligand in arr {
        let id = ligand.get("ligandId").and_then(|i| i.as_u64()).map(|n| n.to_string()).ok_or_else(|| ScienceError::Invalid("gtopdb: missing ligandId".into()))?;
        let name = ligand.get("name").and_then(|n| n.as_str()).unwrap_or(&id).to_owned();
        let t = ligand.get("type").and_then(|t| t.as_str()).unwrap_or("");
        records.push(RetrievedRecord {
            id: id.clone(), title: name, container: format!("GtoPdb {t}").trim().to_owned(),
            url: format!("https://www.guidetopharmacology.org/GRAC/LigandDisplayForward?ligandId={id}"),
        });
    }
    Ok(ParsedResponse { total_hits: records.len() as u64, records })
}

pub struct GtopdbAdapter;
impl ProtocolAdapter for GtopdbAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::GTOPDB }
    fn expected_exchanges(&self) -> usize { 1 }
    fn build_fixture_paths(&self, q: &str, _m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![search_path(q)]) }
    fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> {
        if e.len() != 1 { return Err(ScienceError::Invalid("gtopdb: requires 1 exchange".into())); }
        parse_search(&e[0].response)
    }
}

#[cfg(test)] mod tests { use super::*;
    const F: &[u8] = br#"[{"ligandId":123,"name":"Aspirin","type":"Synthetic organic"}]"#;
    #[test] fn parse_ok() { let p = parse_search(F).unwrap(); assert_eq!(p.total_hits,1); assert_eq!(p.records[0].id,"123"); }
    #[test] fn bad() { assert!(parse_search(b"{}").is_err()); }
}
