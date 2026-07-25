//! ChEBI via OLS4 API. Seam: S3.
use super::adapter::ProtocolAdapter;
use super::fetch::{ParsedResponse, RetrievedRecord};
use crate::ScienceError;

pub fn search_path(query: &str, max: u32) -> String { format!("/ols4/api/search?q={}&ontology=chebi&rows={}", super::url_encode(query), max.clamp(1,50)) }

pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let docs = v.pointer("/response/docs").and_then(|d| d.as_array())
        .ok_or_else(|| ScienceError::Invalid("chebi: missing response/docs".into()))?;
    let total = v.pointer("/response/numFound").and_then(|n| n.as_u64()).unwrap_or(docs.len() as u64);
    let mut records = Vec::with_capacity(docs.len());
    for doc in docs {
        let id = doc.get("obo_id").and_then(|o| o.as_str()).or_else(|| doc.get("short_form").and_then(|s| s.as_str())).filter(|i| !i.is_empty())
            .ok_or_else(|| ScienceError::Invalid("chebi: missing id".into()))?;
        let title = doc.get("label").and_then(|l| l.as_str()).unwrap_or(id).to_owned();
        records.push(RetrievedRecord {
            id: id.to_owned(), title, container: "ChEBI".to_owned(),
            url: format!("https://www.ebi.ac.uk/chebi/searchId.do?chebiId={id}"),
        });
    }
    Ok(ParsedResponse { total_hits: total, records })
}

pub struct ChebiAdapter;
impl ProtocolAdapter for ChebiAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::CHEBI }
    fn expected_exchanges(&self) -> usize { 1 }
    fn build_fixture_paths(&self, q: &str, m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![search_path(q, m)]) }
    fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> {
        if e.len() != 1 { return Err(ScienceError::Invalid("chebi: requires 1 exchange".into())); }
        parse_search(&e[0].response)
    }
}

#[cfg(test)] mod tests { use super::*;
    const F: &[u8] = br#"{"response":{"numFound":1,"docs":[{"obo_id":"CHEBI:15365","label":"aspirin"}]}}"#;
    #[test] fn parse_ok() { let p = parse_search(F).unwrap(); assert_eq!(p.total_hits,1); assert_eq!(p.records[0].id,"CHEBI:15365"); }
    #[test] fn bad() { assert!(parse_search(b"{}").is_err()); }
}
