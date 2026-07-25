//! PDBe Solr search API. Seam contract: S3.
use super::adapter::ProtocolAdapter;
use super::fetch::{ParsedResponse, RetrievedRecord};
use crate::ScienceError;

const FL: &str = "pdb_id,title,experimental_method,resolution,organism_scientific_name";

pub fn search_path(query: &str, max: u32) -> String {
    let rows = max.clamp(1, 50);
    format!("/pdbe/search/pdb/select?q={}&wt=json&rows={rows}&fl={FL}", super::url_encode(query))
}

pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let docs = v.pointer("/response/docs").and_then(|d| d.as_array())
        .ok_or_else(|| ScienceError::Invalid("pdbe: missing response/docs".into()))?;
    let total = v.pointer("/response/numFound").and_then(|n| n.as_u64()).unwrap_or(docs.len() as u64);
    let mut records = Vec::with_capacity(docs.len());
    for doc in docs {
        let id = doc.get("pdb_id").and_then(|i| i.as_str()).filter(|i| !i.is_empty())
            .ok_or_else(|| ScienceError::Invalid("pdbe: missing pdb_id".into()))?;
        let title = doc.get("title").and_then(|t| t.as_str()).unwrap_or("(untitled)");
        let container = doc.get("organism_scientific_name").and_then(|o| o.as_array())
            .and_then(|a| a.first()).and_then(|o| o.as_str()).unwrap_or("PDB");
        records.push(RetrievedRecord {
            id: id.to_owned(), title: title.to_owned(), container: container.to_owned(),
            url: format!("https://www.ebi.ac.uk/pdbe/entry/pdb/{id}"),
        });
    }
    Ok(ParsedResponse { total_hits: total, records })
}

pub struct PdbeAdapter;
impl ProtocolAdapter for PdbeAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::PDBE }
    fn expected_exchanges(&self) -> usize { 1 }
    fn build_fixture_paths(&self, q: &str, m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![search_path(q, m)]) }
    fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> {
        if e.len() != 1 { return Err(ScienceError::Invalid("pdbe: requires 1 exchange".into())); }
        parse_search(&e[0].response)
    }
}

#[cfg(test)] mod tests { use super::*;
    const F: &[u8] = br#"{"response":{"numFound":1,"docs":[{"pdb_id":"4hhb","title":"HUMAN DEOXYHAEMOGLOBIN","experimental_method":["X-RAY DIFFRACTION"],"organism_scientific_name":["Homo sapiens"]}]}}"#;
    #[test] fn parse_ok() { let p = parse_search(F).unwrap(); assert_eq!(p.total_hits,1); assert_eq!(p.records[0].id,"4hhb"); }
    #[test] fn bad() { assert!(parse_search(b"{}").is_err()); }
}
