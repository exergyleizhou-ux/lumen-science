//! PubChem PUG REST API. Seam: S3. 2-step: name→CIDs, CIDs→properties.
use super::adapter::ProtocolAdapter;
use super::fetch::{ParsedResponse, RetrievedRecord};
use crate::ScienceError;
const PROPS: &str = "Title,MolecularFormula,MolecularWeight";

pub fn cids_path(query: &str) -> String { format!("/rest/pug/compound/name/{}/cids/JSON?name_type=word", super::url_encode(query)) }
pub fn props_path(cids: &[String], max: u32) -> String {
    let csv: String = cids.iter().take(max as usize).map(|c| c.as_str()).collect::<Vec<_>>().join(",");
    format!("/rest/pug/compound/cid/{csv}/property/{PROPS}/JSON")
}

pub fn parse_cids(bytes: &[u8]) -> crate::Result<Vec<String>> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let list = v.pointer("/IdentifierList/CID").and_then(|l| l.as_array()).ok_or_else(|| ScienceError::Invalid("pubchem: missing CID list".into()))?;
    Ok(list.iter().filter_map(|c| c.as_u64().map(|n| n.to_string())).collect())
}

pub fn parse_props(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let props = v.pointer("/PropertyTable/Properties").and_then(|p| p.as_array()).ok_or_else(|| ScienceError::Invalid("pubchem: missing Properties".into()))?;
    let mut records = Vec::with_capacity(props.len());
    for p in props {
        let cid = p.get("CID").and_then(|c| c.as_u64()).map(|n| n.to_string()).ok_or_else(|| ScienceError::Invalid("pubchem: missing CID".into()))?;
        let title = p.get("Title").and_then(|t| t.as_str()).unwrap_or(&cid).to_owned();
        records.push(RetrievedRecord {
            id: cid.clone(), title, container: "PubChem".to_owned(),
            url: format!("https://pubchem.ncbi.nlm.nih.gov/compound/{cid}"),
        });
    }
    Ok(ParsedResponse { total_hits: records.len() as u64, records })
}

pub struct PubchemAdapter;
impl ProtocolAdapter for PubchemAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::PUBCHEM }
    fn expected_exchanges(&self) -> usize { 2 }
    fn build_fixture_paths(&self, q: &str, m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> {
        Ok(vec![cids_path(q), props_path(&[], m)])
    }
    fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> {
        if e.len() != 2 { return Err(ScienceError::Invalid("pubchem: requires 2 exchanges".into())); }
        let cids = parse_cids(&e[0].response)?;
        if cids.is_empty() { return Err(ScienceError::Invalid("pubchem: no CIDs found".into())); }
        parse_props(&e[1].response)
    }
}

#[cfg(test)] mod tests { use super::*;
    const CIDS: &[u8] = br#"{"IdentifierList":{"CID":[2244]}}"#;
    const PROPS_DATA: &[u8] = br#"{"PropertyTable":{"Properties":[{"CID":2244,"Title":"ASPIRIN","MolecularFormula":"C9H8O4","MolecularWeight":180.16}]}}"#;
    #[test] fn parse_cids_ok() { assert_eq!(parse_cids(CIDS).unwrap(), vec!["2244"]); }
    #[test] fn parse_props_ok() { let p = parse_props(PROPS_DATA).unwrap(); assert_eq!(p.records[0].id,"2244"); }
    #[test] fn bad() { assert!(parse_cids(b"{}").is_err()); assert!(parse_props(b"{}").is_err()); }
}
