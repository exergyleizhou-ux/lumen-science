//! AlphaFold DB API. Seam contract: S3.
use super::adapter::ProtocolAdapter;
use super::fetch::{ParsedResponse, RetrievedRecord};
use crate::ScienceError;

pub fn prediction_path(accession: &str) -> String {
    format!("/api/prediction/{}", accession)
}

pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    // AlphaFold returns array of predictions
    let arr = if let Some(arr) = v.as_array() { arr.clone() } else { vec![v] };
    let mut records = Vec::with_capacity(arr.len());
    for entry in &arr {
        let id = entry.get("entryId").and_then(|i| i.as_str()).or_else(|| entry.get("uniprotAccession").and_then(|a| a.as_str()))
            .filter(|i| !i.is_empty()).ok_or_else(|| ScienceError::Invalid("alphafold: missing id".into()))?;
        let title = entry.get("uniprotDescription").and_then(|t| t.as_str()).unwrap_or(id);
        let container = entry.get("organismScientificName").and_then(|o| o.as_str()).unwrap_or("(unknown)");
        let plddt = entry.get("globalMetricValue").and_then(|p| p.as_f64()).map(|p| format!("pLDDT:{p:.0}")).unwrap_or_default();
        records.push(RetrievedRecord {
            id: id.to_owned(), title: format!("{title} {plddt}").trim().to_owned(),
            container: container.to_owned(),
            url: entry.get("entryId").and_then(|i| i.as_str()).map(|i| format!("https://alphafold.ebi.ac.uk/entry/{i}")).unwrap_or_default(),
        });
    }
    Ok(ParsedResponse { total_hits: records.len() as u64, records })
}

pub struct AlphafoldAdapter;
impl ProtocolAdapter for AlphafoldAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::ALPHAFOLD }
    fn expected_exchanges(&self) -> usize { 1 }
    fn build_fixture_paths(&self, q: &str, _m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![prediction_path(q)]) }
    fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> {
        if e.len() != 1 { return Err(ScienceError::Invalid("alphafold: requires 1 exchange".into())); }
        parse_search(&e[0].response)
    }
}

#[cfg(test)] mod tests { use super::*;
    const F: &[u8] = br#"[{"entryId":"AF-P01308-F1","uniprotAccession":"P01308","uniprotDescription":"Insulin","organismScientificName":"Homo sapiens","gene":"INS","globalMetricValue":92.5}]"#;
    #[test] fn parse_ok() { let p = parse_search(F).unwrap(); assert_eq!(p.total_hits,1); assert!(p.records[0].title.contains("Insulin")); }
    #[test] fn bad() { assert!(parse_search(b"not json").is_err()); }
}
