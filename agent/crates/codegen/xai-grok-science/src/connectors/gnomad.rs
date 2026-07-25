//! gnomAD GraphQL API. Seam: S3. POST single-entity lookup.
use super::adapter::ProtocolAdapter; use super::fetch::{ParsedResponse, RetrievedRecord}; use crate::ScienceError;
pub fn parse_gene(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let data = v.get("data").ok_or_else(|| ScienceError::Invalid("gnomad: missing data".into()))?;
    if let Some(g) = data.get("gene") {
        let id = g.get("gene_id").and_then(|i| i.as_str()).ok_or_else(|| ScienceError::Invalid("gnomad: missing gene_id".into()))?;
        let name = g.get("symbol").and_then(|s| s.as_str()).unwrap_or(id);
        Ok(ParsedResponse { total_hits: 1, records: vec![RetrievedRecord { id: id.to_owned(), title: name.to_owned(), container: "gnomAD".to_owned(), url: format!("https://gnomad.broadinstitute.org/gene/{id}") }] })
    } else { Err(ScienceError::Invalid("gnomad: no gene data".into())) }
}
pub struct GnomadAdapter;
impl ProtocolAdapter for GnomadAdapter { fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::GNOMAD } fn expected_exchanges(&self) -> usize { 1 } fn build_fixture_paths(&self, q: &str, _m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![format!("/?query={{%20gene(gene_id:%20\"{}\")%20{{%20gene_id,symbol%20}}%20}}", q)]) } fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> { if e.len() != 1 { return Err(ScienceError::Invalid("gnomad: 1 exchange".into())); } parse_gene(&e[0].response) } }
#[cfg(test)] mod tests { use super::*; const F: &[u8] = br#"{"data":{"gene":{"gene_id":"ENSG00000139618","symbol":"BRCA2"}}}"#; #[test] fn ok() { let p = parse_gene(F).unwrap(); assert_eq!(p.records[0].title,"BRCA2"); } }
