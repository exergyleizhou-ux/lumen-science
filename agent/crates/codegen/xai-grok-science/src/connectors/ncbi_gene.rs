//! NCBI Gene via E-utilities. Seam: S3. 2-step: esearch + esummary.
use super::adapter::ProtocolAdapter; use super::fetch::{ParsedResponse, RetrievedRecord}; use crate::ScienceError;
pub fn esearch_path(term: &str, max: u32) -> String { format!("/esearch.fcgi?db=gene&retmode=json&retmax={max}&retstart=0&term={}", super::url_encode(term)) }
pub fn esummary_path(ids: &[String]) -> String { format!("/esummary.fcgi?db=gene&retmode=json&id={}", ids.join(",")) }
fn parse_esearch(bytes: &[u8]) -> crate::Result<Vec<String>> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let list = v.pointer("/esearchresult/idlist").and_then(|l| l.as_array()).ok_or_else(|| ScienceError::Invalid("ncbi-gene: missing idlist".into()))?;
    Ok(list.iter().filter_map(|i| i.as_str().map(|s| s.to_owned())).collect())
}
fn parse_esummary(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let r = v.get("result").ok_or_else(|| ScienceError::Invalid("ncbi-gene: missing result".into()))?;
    let uids = r.get("uids").and_then(|u| u.as_array()).ok_or_else(|| ScienceError::Invalid("ncbi-gene: missing uids".into()))?;
    let mut records = Vec::with_capacity(uids.len());
    for uid in uids {
        let uid = uid.as_str().ok_or_else(|| ScienceError::Invalid("ncbi-gene: non-string uid".into()))?;
        let item = r.get(uid).ok_or_else(|| ScienceError::Invalid(format!("ncbi-gene: missing {uid}").into()))?;
        let name = item.get("name").and_then(|n| n.as_str()).or_else(|| item.get("nomenclaturesymbol").and_then(|n| n.as_str())).unwrap_or(uid);
        let desc = item.get("description").and_then(|d| d.as_str()).or_else(|| item.get("nomenclaturename").and_then(|n| n.as_str())).unwrap_or("");
        let org = item.get("organism").and_then(|o| o.get("scientificname")).and_then(|s| s.as_str()).unwrap_or("");
        records.push(RetrievedRecord { id: uid.to_owned(), title: format!("{name} {desc}").trim().to_owned(), container: org.to_owned(), url: format!("https://www.ncbi.nlm.nih.gov/gene/{uid}") });
    }
    Ok(ParsedResponse { total_hits: records.len() as u64, records })
}
pub struct NcbiGeneAdapter;
impl ProtocolAdapter for NcbiGeneAdapter { fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::NCBI_GENE } fn expected_exchanges(&self) -> usize { 2 } fn build_fixture_paths(&self, q: &str, m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![esearch_path(q, m), esummary_path(&[])]) }
    fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> { if e.len() != 2 { return Err(ScienceError::Invalid("ncbi-gene: 2 exchanges".into())); } let ids = parse_esearch(&e[0].response)?; if ids.is_empty() { return Err(ScienceError::Invalid("ncbi-gene: no ids".into())); } parse_esummary(&e[1].response) } }
#[cfg(test)] mod tests { use super::*; const S: &[u8] = br#"{"esearchresult":{"idlist":["672"]}}"#; const M: &[u8] = br#"{"result":{"uids":["672"],"672":{"name":"BRCA1","description":"BRCA1 DNA repair associated","organism":{"scientificname":"Homo sapiens"}}}}"#;
    #[test] fn ok() { let ids = parse_esearch(S).unwrap(); assert_eq!(ids, vec!["672"]); let p = parse_esummary(M).unwrap(); assert_eq!(p.records[0].id,"672"); }
}
