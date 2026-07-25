//! BindingDB REST API. Seam: S3.
use super::adapter::ProtocolAdapter;
use super::fetch::{ParsedResponse, RetrievedRecord};
use crate::ScienceError;

pub fn ligands_path(accession: &str) -> String { format!("/rest/getLigandsByUniprots?uniprot={}&cutoff=10000&code=0&response=application/json", accession) }

pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let affinities = v.as_object().and_then(|o| o.values().find_map(|val| val.as_object()?.get("affinities")?.as_array()))
        .or_else(|| v.get("affinities").and_then(|a| a.as_array()))
        .ok_or_else(|| ScienceError::Invalid("bindingdb: missing affinities".into()))?;
    let mut records = Vec::with_capacity(affinities.len());
    for a in affinities {
        let mid = a.get("monomerid").and_then(|m| m.as_u64()).map(|n| n.to_string()).unwrap_or_default();
        if mid.is_empty() { continue; }
        let target = a.get("query").and_then(|q| q.as_str()).unwrap_or("");
        let title = format!("{target}").trim().to_owned();
        records.push(RetrievedRecord {
            id: mid.clone(), title, container: "BindingDB".to_owned(),
            url: format!("https://www.bindingdb.org/rwd/bind/chemsearch/marvin/MolStructure.jsp?monomerid={mid}"),
        });
    }
    Ok(ParsedResponse { total_hits: records.len() as u64, records })
}

pub struct BindingdbAdapter;
impl ProtocolAdapter for BindingdbAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::BINDINGDB }
    fn expected_exchanges(&self) -> usize { 1 }
    fn build_fixture_paths(&self, q: &str, _m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![ligands_path(q)]) }
    fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> {
        if e.len() != 1 { return Err(ScienceError::Invalid("bindingdb: requires 1 exchange".into())); }
        parse_search(&e[0].response)
    }
}

#[cfg(test)] mod tests { use super::*;
    const F: &[u8] = br#"{"getLigandsByUniprotsResponse":{"affinities":[{"monomerid":12345,"query":"P01308","affinity_type":"IC50","affinity":"10 nM"}]}}"#;
    #[test] fn parse_ok() { let p = parse_search(F).unwrap(); assert_eq!(p.total_hits,1); assert_eq!(p.records[0].id,"12345"); }
    #[test] fn bad() { assert!(parse_search(b"{}").is_err()); }
}
