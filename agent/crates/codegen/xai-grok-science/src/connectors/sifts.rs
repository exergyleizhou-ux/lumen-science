//! SIFTS mappings API. Seam contract: S3.
use super::adapter::ProtocolAdapter;
use super::fetch::{ParsedResponse, RetrievedRecord};
use crate::ScienceError;

pub fn best_structures_path(accession: &str) -> String {
    format!("/pdbe/api/mappings/best_structures/{}", accession)
}

pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    // Empty array: no matching structures (valid, but zero results).
    if v.as_array().map(|a| a.is_empty()).unwrap_or(false) {
        return Ok(ParsedResponse { total_hits: 0, records: vec![] });
    }
    // Normal response: object keyed by UniProt accession.
    let obj = v.as_object().ok_or_else(|| ScienceError::Invalid("sifts: not an object".into()))?;
    let mut records = Vec::new();
    for (_key, val) in obj {
        let empty = vec![];
        let arr = val.as_array().unwrap_or(&empty);
        for entry in arr {
            let pdb = entry.get("pdb_id").and_then(|p| p.as_str()).unwrap_or("");
            let chain = entry.get("chain_id").and_then(|c| c.as_str()).unwrap_or("");
            let id = if !pdb.is_empty() { format!("{pdb}_{chain}") } else { continue };
            let method = entry.get("experimental_method").and_then(|m| m.as_str()).unwrap_or("");
            let res = entry.get("resolution").and_then(|r| r.as_f64()).map(|r| format!("{r:.1}A")).unwrap_or_default();
            let cov = entry.get("coverage").and_then(|c| c.as_f64()).map(|c| format!("coverage:{c:.1}%%")).unwrap_or_default();
            records.push(RetrievedRecord {
                id: id.clone(), title: format!("{method} {res} {cov}").trim().to_owned(),
                container: "SIFTS".to_owned(),
                url: format!("https://www.ebi.ac.uk/pdbe/entry/pdb/{pdb}"),
            });
        }
    }
    Ok(ParsedResponse { total_hits: records.len() as u64, records })
}

pub struct SiftsAdapter;
impl ProtocolAdapter for SiftsAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::SIFTS }
    fn expected_exchanges(&self) -> usize { 1 }
    fn build_fixture_paths(&self, q: &str, _m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![best_structures_path(q)]) }
    fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> {
        if e.len() != 1 { return Err(ScienceError::Invalid("sifts: requires 1 exchange".into())); }
        parse_search(&e[0].response)
    }
}

#[cfg(test)] mod tests { use super::*;
    const F: &[u8] = br#"{"P01308":[{"pdb_id":"4hhb","chain_id":"A","experimental_method":"X-ray diffraction","resolution":1.8,"coverage":0.95}]}"#;
    #[test] fn parse_ok() { let p = parse_search(F).unwrap(); assert_eq!(p.total_hits,1); assert!(p.records[0].id.contains("4hhb")); }
    #[test] fn bad() { assert!(parse_search(b"not json").is_err()); assert!(parse_search(b"[]").is_ok()); }
}
