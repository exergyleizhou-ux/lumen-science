//! Open Targets GraphQL API. Seam: S3. POST.
use super::adapter::ProtocolAdapter; use super::fetch::{ParsedResponse, RetrievedRecord}; use crate::ScienceError;
pub fn search_body(query: &str, max: u32) -> String { let size = max.clamp(1,50); format!(r#"{{"query":"query Search($q:String!,$size:Int!){{search(queryString:$q,entityNames:[\"target\",\"disease\",\"drug\"],page:{{index:0,size:$size}}){{hits{{id name entity description}}}}}}","variables":{{"q":"{query}","size":{size}}}}}"#) }
pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let hits = v.pointer("/data/search/hits").and_then(|h| h.as_array()).ok_or_else(|| ScienceError::Invalid("opentargets: missing hits".into()))?;
    let mut records = Vec::with_capacity(hits.len());
    for hit in hits {
        let id = hit.get("id").and_then(|i| i.as_str()).filter(|i| !i.is_empty()).ok_or_else(|| ScienceError::Invalid("opentargets: missing id".into()))?;
        let name = hit.get("name").and_then(|n| n.as_str()).unwrap_or(id);
        let entity = hit.get("entity").and_then(|e| e.as_str()).unwrap_or("");
        records.push(RetrievedRecord { id: id.to_owned(), title: name.to_owned(), container: entity.to_owned(), url: format!("https://platform.opentargets.org/{entity}/{id}") });
    }
    Ok(ParsedResponse { total_hits: records.len() as u64, records })
}
pub struct OpentargetsAdapter;
impl ProtocolAdapter for OpentargetsAdapter { fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::OPENTARGETS } fn expected_exchanges(&self) -> usize { 1 } fn build_fixture_paths(&self, q: &str, m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![format!("/api/v4/graphql?{}", super::url_encode(&search_body(q, m)))]) } fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> { if e.len() != 1 { return Err(ScienceError::Invalid("opentargets: 1 exchange".into())); } parse_search(&e[0].response) } }
#[cfg(test)] mod tests { use super::*; const F: &[u8] = br#"{"data":{"search":{"hits":[{"id":"ENSG00000139618","name":"BRCA2","entity":"target","description":"Breast cancer type 2 susceptibility protein"}]}}}"#; #[test] fn ok() { let p = parse_search(F).unwrap(); assert_eq!(p.total_hits,1); } }
